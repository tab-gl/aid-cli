import fs from "node:fs/promises";
import path from "node:path";

const SDK_PACKAGE = "ace-id-sdk";
const VANILLA_SDK_FILE = "ace-id-sdk.global.js";
const VANILLA_SDK_VERSION = "0.1.2";
const VANILLA_SDK_URL =
  `https://unpkg.com/${SDK_PACKAGE}@${VANILLA_SDK_VERSION}/dist/${VANILLA_SDK_FILE}`;

// Detection is intentionally scoped to process.cwd().
// Never walk upward, inspect parent folders, or infer a project from an enclosing workspace.
export async function detectProject() {
  const root = process.cwd();
  const pkg = await readJson(path.join(root, "package.json"));
  const files = new Set(await readTopLevelFiles(root));

  if (await isAndroidProject(root, files)) {
    const language = await detectAndroidLanguage(root);
    return {
      type: language === "kotlin" ? "android-kotlin" : "android-java",
      label: language === "kotlin" ? "Android (Kotlin)" : "Android (Java)",
      sdk: "ace-id-sdk-android",
      sdkMode: "android",
      packageManager: null
    };
  }

  const deps = {
    ...(pkg?.dependencies || {}),
    ...(pkg?.devDependencies || {}),
    ...(pkg?.peerDependencies || {})
  };

  if (
    deps.next ||
    files.has("next.config.js") ||
    files.has("next.config.mjs") ||
    files.has("next.config.ts")
  ) {
    return {
      type: "next",
      label: "Next.js",
      sdk: SDK_PACKAGE,
      sdkMode: "build",
      packageManager: detectPackageManager(files)
    };
  }

  if (
    deps.vite ||
    files.has("vite.config.js") ||
    files.has("vite.config.mjs") ||
    files.has("vite.config.ts")
  ) {
    return {
      type: "vite",
      label: deps.react ? "React + Vite" : "Vite",
      sdk: SDK_PACKAGE,
      sdkMode: "build",
      packageManager: detectPackageManager(files)
    };
  }

  if (
    deps.react ||
    deps["react-dom"] ||
    (files.has("src") && await hasReactSource(root))
  ) {
    return {
      type: "react",
      label: "React",
      sdk: SDK_PACKAGE,
      sdkMode: "build",
      packageManager: detectPackageManager(files)
    };
  }

  return {
    type: "vanilla",
    label: "Vanilla JavaScript",
    sdk: SDK_PACKAGE,
    sdkMode: "vanilla",
    sdkUrl: VANILLA_SDK_URL,
    sdkVersion: VANILLA_SDK_VERSION,
    packageManager: null
  };
}

function detectPackageManager(files) {
  if (files.has("pnpm-lock.yaml")) return "pnpm";
  if (files.has("yarn.lock")) return "yarn";
  if (files.has("bun.lockb") || files.has("bun.lock")) return "bun";
  return "npm";
}

async function isAndroidProject(root, files) {
  if (
    await exists(path.join(root, "app", "src", "main", "AndroidManifest.xml")) ||
    files.has("gradlew")
  ) {
    return true;
  }

  for (const file of [
    "settings.gradle",
    "settings.gradle.kts",
    "build.gradle",
    "build.gradle.kts"
  ]) {
    const content = await readText(path.join(root, file));
    if (
      content &&
      (
        /com\.android\.(application|library)/.test(content) ||
        /com\.android\.tools\.build:gradle/.test(content) ||
        /include\s*\(?\s*["']?:app/.test(content)
      )
    ) {
      return true;
    }
  }

  return false;
}

async function readTopLevelFiles(root) {
  try {
    return await fs.readdir(root);
  } catch {
    return [];
  }
}

async function readJson(file) {
  try {
    return JSON.parse(await fs.readFile(file, "utf8"));
  } catch {
    return null;
  }
}

async function readText(file) {
  try {
    return await fs.readFile(file, "utf8");
  } catch {
    return "";
  }
}

async function exists(file) {
  try {
    await fs.access(file);
    return true;
  } catch {
    return false;
  }
}

async function detectAndroidLanguage(root) {
  const roots = [
    path.join(root, "app", "src", "main"),
    path.join(root, "src", "main")
  ];

  let java = false;
  let kotlin = false;

  for (const sourceRoot of roots) {
    const files = await findSourceFiles(sourceRoot);
    java ||= files.some((file) => file.endsWith(".java"));
    kotlin ||= files.some((file) => file.endsWith(".kt") || file.endsWith(".kts"));
  }

  return kotlin ? "kotlin" : java ? "java" : "kotlin";
}

async function findSourceFiles(root) {
  const results = [];

  async function walk(dir) {
    let entries;
    try {
      entries = await fs.readdir(dir, { withFileTypes: true });
    } catch {
      return;
    }

    for (const entry of entries) {
      if (entry.name === "build" || entry.name === ".gradle") continue;
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        await walk(full);
      } else if (entry.isFile()) {
        results.push(full);
      }
    }
  }

  await walk(root);
  return results;
}

async function hasReactSource(root) {
  const candidates = [
    "src/App.jsx",
    "src/App.tsx",
    "src/main.jsx",
    "src/main.tsx"
  ];

  for (const file of candidates) {
    if (await exists(path.join(root, file))) return true;
  }

  return false;
}
