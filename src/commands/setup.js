import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { parseArgs } from "node:util";
import { createInterface } from "node:readline/promises";

import { loadTokens } from "../auth/tokens.js";
import { login } from "../auth/login.js";
import { apiJson, apiFetch } from "../api/client.js";
import { ISSUER, SCOPES, REQUEST_TIMEOUT_MS } from "../auth/config.js";
import { success, fields, warn, heading } from "../ui/format.js";
import { dim } from "../ui/colors.js";
import { showBanner } from "../ui/banner.js";
import { detectProject } from "../project/detect.js";

const execFileAsync = promisify(execFile);
const CONFIG_FILE = ".aid.json";
const RECOVERY_FILE = ".aid-setup-recovery.json";
const ANDROID_SDK_REPO = "aceyash-dev/ace-id-sdk";
const ANDROID_SDK_RELEASE = "1.1";
const ANDROID_SDK_ASSET = "library-release.aar";
const ANDROID_SDK_SHA256 = "8fde79fd7690f6b09c1198fec08ff160f5ecaf54301f14b88c4f43de90ebdba2";

export async function setup(args = []) {
  let values;
  try {
    ({ values } = parseArgs({ args, options: {
      name: { type: "string", short: "n" }, redirect: { type: "string", short: "r" },
      scope: { type: "string", short: "s", multiple: true }, force: { type: "boolean", short: "f" },
      interactive: { type: "boolean", short: "i" }, wizard: { type: "boolean", short: "w" },
      recover: { type: "boolean" }
    }, strict: true }));
  } catch {
    throw new Error("Usage: aid setup [--interactive] [--name <name>] [--redirect <uri>] [--scope <scope>...] [--force]");
  }

  showBanner();
  const project = await detectProject();
  const configPath = path.join(process.cwd(), CONFIG_FILE);
  const recoveryPath = path.join(process.cwd(), RECOVERY_FILE);
  if (values.recover) {
    await recoverSetup(configPath, recoveryPath);
    return;
  }
  const interactive = Boolean(values.interactive || values.wizard);

  if (interactive && (!process.stdin.isTTY || !process.stdout.isTTY)) {
    throw new Error("Interactive setup requires a terminal. Use `aid setup` for the fast non-interactive flow.");
  }

  if (await fileExists(configPath) && !values.force) {
    if (!interactive) {
      success("Ace ID is already configured for this project.");
      console.log(`  ${dim("CONFIG")}  ${CONFIG_FILE}`);
      console.log(`  ${dim("RE-RUN")}  aid setup --force`);
      return;
    }
    if (!(await confirm("An existing .aid.json was found. Replace its configuration? [y/N] "))) {
      console.log("Setup cancelled.");
      return;
    }
  }

  heading(interactive ? "SETUP WIZARD" : "SETUP");
  console.log(`\n  ${dim("DETECTED")}  ${project.label}${
    project.sdk ? `  •  ${project.sdk}` : "  •  native SDK not installed"
  }\n`);

  let tokens = await loadTokens();
  if (!tokens?.access_token) {
    if (!process.stdout.isTTY || !process.stdin.isTTY) {
      throw new Error("Not signed in. Run `aid login` first.");
    }
    await login();
    tokens = await loadTokens();
    if (!tokens?.access_token) throw new Error("Sign-in completed without a usable session.");
  }

  const defaults = {
    name: values.name?.trim() || await detectProjectName(),
    redirect: values.redirect?.trim() || "http://localhost:3000/callback",
    scopes: normalizeScopes(values.scope?.flat() || SCOPES)
  };
  const settings = interactive ? await collectWizardValues(defaults) : defaults;

  const app = await apiJson("/apps", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ name: settings.name })
  });

  let recovery = {
    issuer: ISSUER,
    app_id: app.id,
    client_id: null,
    created_at: new Date().toISOString(),
    stage: "app-created"
  };
  try {
    await writeRecovery(recoveryPath, recovery);
  } catch (err) {
    const cleanupFailures = await cleanupRemoteResources(app?.id, null);
    if (cleanupFailures.length) {
      warn(`Could not persist setup recovery state. Manually remove app ${app.id}; cleanup also failed: ${cleanupFailures.join("; ")}`);
    }
    throw new Error(`Could not record setup recovery state: ${err.message}`);
  }

  let client;
  try {
    client = await apiJson("/clients", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name: settings.name, redirect_uris: [settings.redirect], scopes: settings.scopes, public: true })
    });
  } catch (err) {
    const cleanupFailures = await cleanupRemoteResources(app?.id, null);
    if (cleanupFailures.length) {
      recovery = { ...recovery, stage: "cleanup-required" };
      await writeRecovery(recoveryPath, recovery).catch(() => {});
      warn(`Setup cleanup is incomplete. Run `aid setup --recover` to retry removal of app ${app.id}.`);
    } else {
      await removeRecovery(recoveryPath);
    }
    throw err;
  }

  recovery = { ...recovery, client_id: client.client_id, stage: "resources-created" };
  try {
    await writeRecovery(recoveryPath, recovery);
  } catch (err) {
    const cleanupFailures = await cleanupRemoteResources(app?.id, client?.client_id);
    if (cleanupFailures.length) {
      warn(`Could not record recovery state. Manually inspect app ${app.id} and client ${client.client_id}; cleanup failed: ${cleanupFailures.join("; ")}`);
    } else {
      await removeRecovery(recoveryPath);
    }
    throw new Error(`Could not record setup recovery state: ${err.message}`);
  }

  let sdkStatus = "not applicable";
  try {
    if (project.sdkMode === "android") sdkStatus = await installAndroidSdk(project);
    else if (project.sdk && project.sdkMode === "build") {
      sdkStatus = await hasDependency(project.sdk) ? "already installed" : await installSdk(project);
    }
  } catch (err) {
    warn(`SDK setup skipped: ${err.message}`);
    sdkStatus = "skipped";
  }

  try {
    await writeConfig(configPath, {
      issuer: ISSUER, app_id: app.id, client_id: client.client_id,
      redirect_uri: settings.redirect, scopes: settings.scopes,
      project_type: project.type, framework: project.label, sdk: project.sdk,
      sdk_path: project.sdkPath, sdk_version: project.sdkVersion
    });
    await removeRecovery(recoveryPath);
  } catch (err) {
    const cleanupFailures = await cleanupRemoteResources(app?.id, client?.client_id);
    if (cleanupFailures.length) {
      recovery = { ...recovery, stage: "cleanup-required" };
      await writeRecovery(recoveryPath, recovery).catch(() => {});
      warn(`Setup cleanup is incomplete. Run `aid setup --recover` to retry removal of the created resources.`);
    } else {
      await removeRecovery(recoveryPath);
    }
    throw new Error(`Could not write ${CONFIG_FILE}: ${err.message}`);
  }

  success("Ace ID project configured.");
  fields([
    ["Project", project.label], ["Application ID", app.id], ["Client ID", client.client_id],
    ["Redirect URI", settings.redirect], ["Scopes", settings.scopes.join(" ")], ["SDK", project.sdk || "none"], ["SDK status", sdkStatus],
    ...(project.sdkPath ? [["SDK path", project.sdkPath]] : []), ["Config", CONFIG_FILE]
  ]);
}

async function collectWizardValues(defaults) {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  try {
    const name = await prompt(rl, "Project name", defaults.name);
    const redirect = await prompt(rl, "Redirect URI", defaults.redirect);
    const scopes = await prompt(rl, "Scopes (space-separated)", defaults.scopes.join(" "));
    if (!name.trim()) throw new Error("Project name cannot be empty.");
    if (!redirect.trim()) throw new Error("Redirect URI cannot be empty.");
    return { name: name.trim(), redirect: redirect.trim(), scopes: normalizeScopes(scopes) };
  } finally {
    rl.close();
  }
}

async function prompt(rl, label, fallback) {
  const answer = await rl.question(`  ${label} [${fallback}]: `);
  return answer.trim() || fallback;
}

async function confirm(message) {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  try {
    const answer = await rl.question(message);
    return /^(y|yes)$/i.test(answer.trim());
  } finally {
    rl.close();
  }
}

function normalizeScopes(scopes) {
  const list = Array.isArray(scopes) ? scopes : String(scopes).split(/\s+/);
  return [...new Set(list.flatMap((value) => String(value).split(/\s+/)).map((value) => value.trim()).filter(Boolean))];
}

async function cleanupRemoteResources(appId, clientId) {
  const failures = [];
  if (clientId) {
    try {
      const response = await apiFetch(`/clients/${encodeURIComponent(clientId)}`, { method: "DELETE" });
      if (!response.ok && response.status !== 404) failures.push(`client ${clientId} (HTTP ${response.status})`);
    } catch (err) {
      failures.push(`client ${clientId} (${err.message})`);
    }
  }
  if (appId) {
    try {
      const response = await apiFetch(`/apps/${encodeURIComponent(appId)}`, { method: "DELETE" });
      if (!response.ok && response.status !== 404) failures.push(`app ${appId} (HTTP ${response.status})`);
    } catch (err) {
      failures.push(`app ${appId} (${err.message})`);
    }
  }
  for (const failure of failures) warn(`Cleanup failed for ${failure}.`);
  return failures;
}

async function writeRecovery(file, state) {
  await writeConfig(file, state);
}

async function removeRecovery(file) {
  await fs.unlink(file).catch((err) => {
    if (err.code !== "ENOENT") throw err;
  });
}

async function recoverSetup(configPath, recoveryPath) {
  let recovery;
  try {
    recovery = JSON.parse(await fs.readFile(recoveryPath, "utf8"));
  } catch (err) {
    if (err.code === "ENOENT") {
      console.log("No interrupted setup recovery record found.");
      return;
    }
    throw new Error(`Setup recovery record is invalid. Inspect ${RECOVERY_FILE} before retrying.`);
  }

  if (recovery?.issuer !== ISSUER || typeof recovery.app_id !== "string" || !recovery.app_id) {
    throw new Error(`Setup recovery record is invalid or belongs to another issuer. Inspect ${RECOVERY_FILE} manually.`);
  }

  let config = null;
  try {
    config = JSON.parse(await fs.readFile(configPath, "utf8"));
  } catch {}
  if (config?.issuer === recovery.issuer && config?.app_id === recovery.app_id &&
      (!recovery.client_id || config?.client_id === recovery.client_id)) {
    await removeRecovery(recoveryPath);
    success("Setup configuration is already complete; cleared the recovery record without deleting resources.");
    return;
  }

  const failures = await cleanupRemoteResources(recovery.app_id, recovery.client_id);
  if (failures.length) {
    throw new Error(`Setup recovery is incomplete. Fix the reported cleanup errors and rerun `aid setup --recover`.`);
  }
  await removeRecovery(recoveryPath);
  success("Removed resources left behind by the interrupted setup.");
}

async function detectProjectName() {
  try {
    const pkg = JSON.parse(await fs.readFile(path.join(process.cwd(), "package.json"), "utf8"));
    if (typeof pkg.name === "string" && pkg.name.trim()) return pkg.name.trim();
  } catch {}
  return path.basename(process.cwd()) || "Ace ID App";
}
async function fileExists(file) {
  try {
    await fs.access(file);
    return true;
  } catch (err) {
    if (err.code === "ENOENT") return false;
    throw err;
  }
}

async function writeConfig(file, config) {
  const tmp = `${file}.${process.pid}.${Date.now()}.tmp`;
  try {
    await fs.writeFile(tmp, JSON.stringify(config, null, 2) + "\n", { mode: 0o600 });
    await fs.chmod(tmp, 0o600);
    await fs.rename(tmp, file);
  } catch (err) {
    await fs.rm(tmp, { force: true }).catch(() => {});
    throw err;
  }
}

async function writeAtomic(file, data, mode = 0o644) {
  const tmp = `${file}.${process.pid}.${Date.now()}.tmp`;
  try {
    await fs.writeFile(tmp, data, { mode });
    await fs.chmod(tmp, mode);
    await fs.rename(tmp, file);
  } catch (err) {
    await fs.rm(tmp, { force: true }).catch(() => {});
    throw err;
  }
}
async function installAndroidSdk(project) {
  const releaseResponse = await fetchWithTimeout(
    `https://api.github.com/repos/${ANDROID_SDK_REPO}/releases/tags/${ANDROID_SDK_RELEASE}`,
    {
      headers: {
        accept: "application/vnd.github+json",
        "user-agent": "ace-id-cli"
      }
    }
  );

  if (!releaseResponse.ok) {
    throw new Error(
      `Could not find Android SDK release ${ANDROID_SDK_RELEASE} on GitHub (HTTP ${releaseResponse.status}).`
    );
  }

  const release = await releaseResponse.json();
  const asset = release.assets?.find(
    (item) => item.name === ANDROID_SDK_ASSET
  );

  if (!asset?.browser_download_url) {
    throw new Error(`Ace ID SDK release ${ANDROID_SDK_RELEASE} does not contain an .aar asset.`);
  }

  if (asset.digest !== `sha256:${ANDROID_SDK_SHA256}`) {
    throw new Error(`Android SDK release ${ANDROID_SDK_RELEASE} digest does not match the pinned artifact.`);
  }

  const sdkDir = path.join(process.cwd(), "libs");
  const sdkPath = path.join(sdkDir, asset.name);
  await fs.mkdir(sdkDir, { recursive: true });

  const response = await fetchWithTimeout(asset.browser_download_url, {
    headers: { "user-agent": "ace-id-cli" }
  });

  if (!response.ok) {
    throw new Error(`Failed to download ${asset.name} (HTTP ${response.status}).`);
  }

  const buffer = Buffer.from(await response.arrayBuffer());
  const digest = crypto.createHash("sha256").update(buffer).digest("hex");
  if (digest !== ANDROID_SDK_SHA256) {
    throw new Error(`Android SDK integrity check failed for ${asset.name}. Installation was aborted.`);
  }

  await writeAtomic(sdkPath, buffer);

  project.sdkPath = path.relative(process.cwd(), sdkPath);
  return "installed";
}

async function hasDependency(name) {
  try {
    const pkg = JSON.parse(
      await fs.readFile(path.join(process.cwd(), "package.json"), "utf8")
    );

    const deps = {
      ...(pkg.dependencies || {}),
      ...(pkg.devDependencies || {}),
      ...(pkg.peerDependencies || {})
    };

    return Boolean(deps[name]);
  } catch {
    return false;
  }
}

async function installSdk(project) {
  const { executable, args } = packageInstallCommand(project.packageManager, project.sdk);

  try {
    const result = await execFileAsync(executableForPlatform(executable), args, {
      cwd: process.cwd(),
      windowsHide: true,
      maxBuffer: 1024 * 1024
    });

    if (result.stdout?.trim()) console.log(result.stdout.trim());
    if (result.stderr?.trim()) console.error(result.stderr.trim());
    return "installed";
  } catch (err) {
    warn(`SDK install was skipped after an install error. ${err.message}`);
    return "install failed";
  }
}

function executableForPlatform(executable) {
  return process.platform === "win32" ? executable + ".cmd" : executable;
}

function packageInstallCommand(manager, packageName) {
  switch (manager) {
    case "pnpm":
      return { executable: "pnpm", args: ["add", packageName] };
    case "yarn":
      return { executable: "yarn", args: ["add", packageName] };
    case "bun":
      return { executable: "bun", args: ["add", packageName] };
    default:
      return { executable: "npm", args: ["install", packageName] };
  }
}

async function fetchWithTimeout(url, options = {}) {
  try {
    // Keep the deadline active through release JSON and artifact body reads.
    return await fetch(url, { ...options, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
  } catch (err) {
    if (err.name === "AbortError" || err.name === "TimeoutError") {
      throw new Error(`SDK download request timed out after ${REQUEST_TIMEOUT_MS / 1000}s.`);
    }
    throw new Error(`SDK download request failed: ${err.message}`);
  }
}
