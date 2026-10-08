import fs from "node:fs/promises";
import path from "node:path";
import { parseArgs } from "node:util";
import { randomUUID } from "node:crypto";

import { apiJson } from "../api/client.js";
import { ISSUER } from "../auth/config.js";
import { detectProject } from "../project/detect.js";
import { success, fields } from "../ui/format.js";

const CONFIG_FILE = ".aid.json";

export async function link(args = []) {
  let parsed;
  try {
    parsed = parseArgs({
      args,
      options: { client: { type: "string", short: "c" }, force: { type: "boolean", short: "f" } },
      strict: true,
      allowPositionals: true
    });
  } catch {
    throw new Error("Usage: aid link <app_id> [--client <client_id>] [--force]");
  }

  const [appId, ...extra] = parsed.positionals;
  if (extra.length) throw new Error("Usage: aid link <app_id> [--client <client_id>] [--force]");
  if (!appId) throw new Error("Usage: aid link <app_id> [--client <client_id>] [--force]");

  const app = await apiJson(`/apps/${encodeURIComponent(appId)}`);
  if (!app?.id) throw new Error(`App ${appId} was returned without an ID.`);

  const configPath = path.join(process.cwd(), CONFIG_FILE);
  if (!parsed.values.force && await fileExists(configPath)) {
    throw new Error(`${CONFIG_FILE} already exists. Use "aid link ${appId} --force" to replace it.`);
  }

  const project = await detectProject();
  const existing = await readConfig(configPath);
  // Keep the local link deterministic: remote app identity is the source of truth.
  const config = {
    issuer: ISSUER,
    app_id: app.id,
    ...(parsed.values.client ? { client_id: parsed.values.client } : existing?.client_id ? { client_id: existing.client_id } : {}),
    project_type: project.type,
    framework: project.label,
    sdk: project.sdk,
    sdk_path: project.sdkPath,
    sdk_version: project.sdkVersion
  };

  await writeConfig(configPath, config);
  success(`Linked ${app.name || app.id} to this folder.`);
  fields([
    ["Application ID", app.id],
    ["Application", app.name || app.id],
    ...(config.client_id ? [["Client ID", config.client_id]] : []),
    ["Config", CONFIG_FILE]
  ]);
}

async function readConfig(file) {
  try { return JSON.parse(await fs.readFile(file, "utf8")); } catch { return null; }
}

async function fileExists(file) {
  try { await fs.access(file); return true; }
  catch (err) { if (err.code === "ENOENT") return false; throw err; }
}

async function writeConfig(file, config) {
  const tmp = `${file}.${process.pid}.${randomUUID()}.tmp`;
  try {
    await fs.writeFile(tmp, JSON.stringify(config, null, 2) + "\n", { mode: 0o600 });
    await fs.chmod(tmp, 0o600);
    await fs.rename(tmp, file);
  } catch (err) {
    await fs.rm(tmp, { force: true }).catch(() => {});
    throw err;
  }
}
