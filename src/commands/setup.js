import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { parseArgs } from "node:util";

import { loadTokens } from "../auth/tokens.js";
import { login } from "../auth/login.js";
import { apiJson, apiFetch } from "../api/client.js";
import { ISSUER, SCOPES } from "../auth/config.js";
import { success, fields, warn, heading } from "../ui/format.js";
import { dim } from "../ui/colors.js";
import { showBanner } from "../ui/banner.js";
import { detectProject } from "../project/detect.js";

const execFileAsync = promisify(execFile);
const CONFIG_FILE = ".aid.json";
const VANILLA_SDK_SHA256 = process.env.ACE_ID_VANILLA_SDK_SHA256 || "";
const ANDROID_SDK_REPO = "aceyash-dev/ace-id-sdk";
const ANDROID_SDK_RELEASE = "1.1";
const ANDROID_SDK_ASSET = "library-release.aar";
const ANDROID_SDK_SHA256 = "8fde79fd7690f6b09c1198fec08ff160f5ecaf54301f14b88c4f43de90ebdba2";

export async function setup(args = []) {
  let values;
  try {
    ({ values } = parseArgs({ args, options: {
      name: { type: "string" }, redirect: { type: "string" },
      scope: { type: "string" }, force: { type: "boolean", short: "f" }
    }, strict: true }));
  } catch {
    throw new Error("Usage: aid setup [--name <name>] [--redirect <uri>] [--scope <scope...>] [--force]");
  }

  showBanner();
  const project = await detectProject();
  const configPath = path.join(process.cwd(), CONFIG_FILE);

  if (await fileExists(configPath) && !values.force) {
    success("Ace ID is already configured for this project.");
    console.log(\`  \${dim("CONFIG")}  \${CONFIG_FILE}\`);
    console.log(\`  \${dim("RE-RUN")}  aid setup --force\`);
    return;
  }

  heading("SETUP");
  console.log(\`\\n  \${dim("DETECTED")}  \${project.label}\${
    project.sdk ? \`  •  \${project.sdk}\` : "  •  native SDK not installed"
  }\\n\`);

  let tokens = await loadTokens();
  if (!tokens?.access_token) {
    if (!process.stdout.isTTY || !process.stdin.isTTY) {
      throw new Error("Not signed in. Run \`aid login\` first.");
    }
    await login();
    tokens = await loadTokens();
    if (!tokens?.access_token) throw new Error("Sign-in completed without a usable session.");
  }

  const projectName = values.name?.trim() || await detectProjectName();
  const redirectUri = values.redirect?.trim() || "http://localhost:3000/callback";
  const scopeList = (values.scope?.trim() || SCOPES).split(/\s+/).filter(Boolean);

  const app = await apiJson("/apps", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ name: projectName })
  });

  let client;
  try {
    client = await apiJson("/clients", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name: projectName, redirect_uris: [redirectUri], scopes: scopeList, public: true })
    });
  } catch (err) {
    try {
      await apiFetch(\`/apps/\${encodeURIComponent(app.id)}\`, { method: "DELETE" });
    } catch {
      warn(\`The client could not be created. App \${app.id} was not automatically removed.\`);
    }
    throw err;
  }

  await writeConfig(configPath, {
    issuer: ISSUER, app_id: app.id, client_id: client.client_id,
    redirect_uri: redirectUri, scopes: scopeList,
    project_type: project.type, framework: project.label, sdk: project.sdk
  });

  let sdkStatus = "not applicable";
  try {
    if (project.sdkMode === "android") sdkStatus = await installAndroidSdk(project);
    else if (project.sdkMode === "vanilla") sdkStatus = await installVanillaSdk(project);
    else if (project.sdk && project.sdkMode === "build") {
      sdkStatus = await hasDependency(project.sdk) ? "already installed" : await installSdk(project);
    }
  } catch (err) {
    warn(\`SDK setup skipped: \${err.message}\`);
    sdkStatus = "skipped";
  }

  success("Ace ID project configured.");
  fields([
    ["Project", project.label], ["Application ID", app.id], ["Client ID", client.client_id],
    ["Redirect URI", redirectUri], ["SDK", project.sdk || "none"], ["SDK status", sdkStatus],
    ...(project.sdkPath ? [["SDK path", project.sdkPath]] : []), ["Config", CONFIG_FILE]
  ]);
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
  const tmp = `${file}.${process.pid}.tmp`;
  await fs.writeFile(tmp, JSON.stringify(config, null, 2) + "\n", { mode: 0o600 });
  await fs.chmod(tmp, 0o600);
  await fs.rename(tmp, file);
}

async function installVanillaSdk(project) {
  const htmlCandidates = ["index.html", "public/index.html"];
  let htmlPath = null;

  for (const candidate of htmlCandidates) {
    try {
      await fs.access(path.join(process.cwd(), candidate));
      htmlPath = path.join(process.cwd(), candidate);
      break;
    } catch {}
  }

  if (!htmlPath) {
    warn("No index.html found. Skipped automatic Vanilla SDK injection.");
    return "skipped";
  }

  let html = await fs.readFile(htmlPath, "utf8");
  if (html.includes("ace-id-sdk.global.js")) return "already installed";

  const response = await fetch(project.sdkUrl, {
    headers: { "user-agent": "ace-id-cli" }
  });
  if (!response.ok) {
    throw new Error(`Failed to download the latest Vanilla SDK (HTTP ${response.status}).`);
  }

  const source = Buffer.from(await response.arrayBuffer());
  const digest = crypto.createHash("sha256").update(source).digest("hex");
  if (!/^[a-f0-9]{64}$/i.test(VANILLA_SDK_SHA256) || digest !== VANILLA_SDK_SHA256.toLowerCase()) {
    throw new Error("Vanilla SDK integrity check failed. Installation was aborted.");
  }

  const sdkDir = path.join(process.cwd(), "vendor");
  const sdkPath = path.join(sdkDir, "ace-id-sdk.global.js");
  await fs.mkdir(sdkDir, { recursive: true });
  await fs.writeFile(sdkPath, source);

  const relative = path.relative(path.dirname(htmlPath), sdkPath).replaceAll(path.sep, "/");
  const script = `  <script src="${relative.startsWith(".") ? relative : "./" + relative}"></script>\n`;
  const head = /<head(?:\s[^>]*)?>/i;
  if (head.test(html)) html = html.replace(head, (match) => `${match}\n${script}`);
  else html = `${script}${html}`;

  await fs.writeFile(htmlPath, html, "utf8");
  project.sdkPath = path.relative(process.cwd(), sdkPath);
  project.sdkUrl = relative;
  return "installed";
}

async function installAndroidSdk(project) {
  const releaseResponse = await fetch(
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

  const response = await fetch(asset.browser_download_url, {
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

  await fs.writeFile(sdkPath, buffer);

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
  const command = packageInstallCommand(project.packageManager, project.sdk);
  const [executable, ...args] = command.split(" ");

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
      return `pnpm add ${packageName}`;
    case "yarn":
      return `yarn add ${packageName}`;
    case "bun":
      return `bun add ${packageName}`;
    default:
      return `npm install ${packageName}`;
  }
}
