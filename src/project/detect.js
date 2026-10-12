import fs from "node:fs/promises";
import path from "node:path";

const SDK_PACKAGE = "ace-id-sdk";
// Detection is intentionally scoped to process.cwd().
// Never walk upward, inspect parent folders, or infer a project from an enclosing workspace.
export async function detectProject() {
  const root = process.cwd();
  const [pkg, topLevelFiles] = await Promise.all([readJson(path.join(root, "package.json")), readTopLevelFiles(root)]);
  const files = new Set(topLevelFiles);

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
    sdkMode: "manual",
    packageManager: detectPackageManager(files)
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
  } catch (err) {
    if (err.code === "ENOENT") return [];
    throw new Error(`Could not inspect project directory: ${err.message}`);
  }
}

async function readJson(file) {
  let raw;
  try {
    raw = await fs.readFile(file, "utf8");
  } catch (err) {
    if (err.code === "ENOENT") return null;
    throw new Error(`Could not read ${path.basename(file)}: ${err.message}`);
  }

  let value;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error(`${path.basename(file)} contains invalid JSON. Fix it before running setup.`);
  }

  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${path.basename(file)} must contain a JSON object.`);
  }

  for (const key of ["dependencies", "devDependencies", "peerDependencies"]) {
    if (value[key] !== undefined &&
        (!value[key] || typeof value[key] !== "object" || Array.isArray(value[key]))) {
      throw new Error(`${path.basename(file)} field "${key}" must be an object.`);
    }
  }

  return value;
}

async function readText(file) {
  try {
    return await fs.readFile(file, "utf8");
  } catch (err) {
    if (err.code === "ENOENT") return "";
    throw new Error(`Could not read ${path.basename(file)}: ${err.message}`);
  }
}

async function exists(file) {
  try {
    await fs.access(file);
    return true;
  } catch (err) {
    if (err.code === "ENOENT") return false;
    throw new Error(`Could not inspect ${path.basename(file)}: ${err.message}`);
  }
}

async function detectAndroidLanguage(root) {
  const roots = [path.join(root, "app", "src", "main"), path.join(root, "src", "main")];
  const found = { java: false, kotlin: false };

  async function walk(dir) {
    if (found.kotlin && found.java) return;
    let entries;
    try { entries = await fs.readdir(dir, { withFileTypes: true }); } catch { return; }
    for (const entry of entries) {
      if (entry.isDirectory()) {
        if (["build", ".gradle", ".git", "node_modules", ".idea"].includes(entry.name)) continue;
        await walk(path.join(dir, entry.name));
        if (found.kotlin && found.java) return;
      } else if (entry.isFile()) {
        found.java ||= entry.name.endsWith(".java");
        found.kotlin ||= entry.name.endsWith(".kt") || entry.name.endsWith(".kts");
        if (found.kotlin && found.java) return;
      }
    }
  }

  await Promise.all(roots.map(walk));
  return found.kotlin ? "kotlin" : found.java ? "java" : "kotlin";
}
async function hasReactSource(root) {
  const candidates = [
    "src/App.jsx",
    "src/App.tsx",
    "src/main.jsx",
    "src/main.tsx"
  ];

  const matches = await Promise.all(candidates.map((file) => exists(path.join(root, file))));
  return matches.some(Boolean);
}
