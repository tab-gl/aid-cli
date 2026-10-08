import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import readline from "node:readline/promises";
import { stdin as input, stdout as output } from "node:process";

import { loadTokens } from "../auth/tokens.js";
import { login } from "../auth/login.js";
import { apiJson } from "../api/client.js";
import { ISSUER, SCOPES } from "../auth/config.js";
import { success, fields, warn, heading } from "../ui/format.js";
import { dim } from "../ui/colors.js";
import { showBanner } from "../ui/banner.js";
import { detectProject } from "../project/detect.js";

const execFileAsync = promisify(execFile);
const CONFIG_FILE = ".aid.json";
const VANILLA_SDK_SHA256 = process.env.ACE_ID_VANILLA_SDK_SHA256 || "";
const ANDROID_SDK_REPO = "aceyash-dev/ace-id-sdk";

export async function setup() {
  if (!input.isTTY || !output.isTTY) {
    throw new Error(
      "Setup needs an interactive terminal. Run `npx ace-id` or `aid setup` from a terminal."
    );
  }

  const rl = readline.createInterface({ input, output });

  try {
    showBanner();

    const project = await detectProject();

    heading("SETUP");

    console.log(`

  Create an Ace ID application and OAuth client for this project.
  Existing CLI credentials are never overwritten.
`);

    console.log(
      `  ${dim("DETECTED")}  ${project.label}${
        project.sdk ? `  •  ${project.sdk}` : "  •  native SDK not installed"
      }
\n`
    );

    let tokens = await loadTokens();

    if (!tokens?.access_token) {
      const shouldLogin = await confirm(
        rl,
        "You are not signed in. Sign in now?",
        true
      );

      if (!shouldLogin) {
        throw new Error("Setup cancelled. Run `aid login` first.");
      }

      await login();
      tokens = await loadTokens();

      if (!tokens?.access_token) {
        throw new Error("Sign-in completed without a usable session.");
      }
    }

    const projectName = await question(
      rl,
      "Project name",
      await detectProjectName()
    );

    const redirectUri = await question(
      rl,
      "Redirect URI",
      "http://localhost:3000/callback"
    );

    const scopes = await question(rl, "Scopes", SCOPES);
    const scopeList = scopes.split(/\s+/).filter(Boolean);

    const app = await apiJson("/apps", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name: projectName })
    });

    const client = await apiJson("/clients", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        name: projectName,
        redirect_uris: [redirectUri],
        scopes: scopeList,
        public: true
      })
    });

    const config = {
      issuer: ISSUER,
      app_id: app.id,
      client_id: client.client_id,
      redirect_uri: redirectUri,
      scopes: scopeList,
      project_type: project.type,
      framework: project.label,
      sdk: project.sdk
    };

    const configPath = path.join(process.cwd(), CONFIG_FILE);

    try {
      await fs.access(configPath);
      const overwrite = await confirm(
        rl,
        `${CONFIG_FILE} already exists. Replace it?`,
        false
      );

      if (!overwrite) {
        warn(`Created the remote resources, but kept the existing ${CONFIG_FILE}.`);
      } else {
        await writeConfig(configPath, config);
      }
    } catch (err) {
      if (err.code !== "ENOENT") throw err;
      await writeConfig(configPath, config);
    }

    let sdkStatus = "not applicable";
    if (project.sdkMode === "android") {
      sdkStatus = await installAndroidSdk(project);
    } else if (project.sdkMode === "vanilla") {
      sdkStatus = await installVanillaSdk(project);
    } else if (project.sdk && project.sdkMode === "build") {
      if (await hasDependency(project.sdk)) {
        sdkStatus = "already installed";
      } else {
        sdkStatus = await installSdk(project);
      }
    }

    success("Ace ID project configured.");

    fields([
      ["Project", project.label],
      ["Application ID", app.id],
      ["Client ID", client.client_id],
      ["Redirect URI", redirectUri],
      ["SDK", project.sdk || "none"],
      ["SDK status", sdkStatus],
      ...(project.sdkPath ? [["SDK path", project.sdkPath]] : []),
      ["Config", CONFIG_FILE]
    ]);

    if (project.sdkMode === "android" && sdkStatus === "installed") {
      console.log(
        `  ${dim("NEXT")}

    Downloaded the latest Ace ID Android AAR to ${project.sdkPath}.
    Add it to your app module dependencies.
`
      );
    } else if (project.sdkMode === "android") {
      console.log(
        `  ${dim("NEXT")}

    Android SDK setup was not completed. The latest AAR can be downloaded from the Ace ID SDK GitHub release.
`
      );
    } else if (project.sdkMode === "vanilla" && sdkStatus === "installed") {
      console.log(
        `  ${dim("NEXT")}\n\n    Installed the Vanilla browser SDK at ${project.sdkPath}.
    It is loaded automatically by your index.html. Use the global \`AceID\` API.
`
      );
    } else if (project.sdkMode === "vanilla") {
      console.log(
        `  ${dim("NEXT")}\n\n    The Vanilla browser SDK was not installed automatically.
`
      );
    } else if (project.sdk && project.sdkMode === "build" && sdkStatus === "installed") {
      console.log(
        `  ${dim("NEXT")}

    The ${project.sdk} package is installed.
    Use the values in ${CONFIG_FILE} to initialize Ace ID.
`
      );
    } else if (project.sdk && project.sdkMode === "build") {
      console.log(
        `  ${dim("NEXT")}

    Install the SDK manually if needed:
    ${packageInstallCommand(project.packageManager, project.sdk)}

    Then initialize it with the values in ${CONFIG_FILE}.
`
      );
    }
  } finally {
    rl.close();
  }
}

async function writeConfig(file, config) {
  await fs.writeFile(
    file,
    JSON.stringify(config, null, 2) + "\n",
    { mode: 0o600 }
  );
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
    `https://api.github.com/repos/${ANDROID_SDK_REPO}/releases/latest`,
    {
      headers: {
        accept: "application/vnd.github+json",
        "user-agent": "ace-id-cli"
      }
    }
  );

  if (!releaseResponse.ok) {
    throw new Error(
      `Could not find the latest Android SDK release on GitHub (HTTP ${releaseResponse.status}).`
    );
  }

  const release = await releaseResponse.json();
  const asset = release.assets?.find(
    (item) => typeof item.name === "string" && item.name.toLowerCase().endsWith(".aar")
  );

  if (!asset?.browser_download_url) {
    throw new Error("The latest Ace ID SDK release does not contain an .aar asset.");
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

async function question(rl, label, fallback) {
  const value = (await rl.question(`  ? ${label} [${fallback}]: `)).trim();
  return value || fallback;
}

async function confirm(rl, label, fallback) {
  const suffix = fallback ? "Y/n" : "y/N";
  const value = (await rl.question(`  ? ${label} [${suffix}]: `))
    .trim()
    .toLowerCase();

  if (!value) return fallback;
  return value === "y" || value === "yes";
}

async function detectProjectName() {
  try {
    const raw = await fs.readFile(
      path.join(process.cwd(), "package.json"),
      "utf8"
    );
    const pkg = JSON.parse(raw);
    if (typeof pkg.name === "string" && pkg.name.trim()) {
      return pkg.name.trim();
    }
  } catch {
    // Fall back to the directory name.
  }

  return path.basename(process.cwd()) || "My App";
}
