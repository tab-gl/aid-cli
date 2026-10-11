import fs from "node:fs/promises";
import path from "node:path";
import { randomUUID } from "node:crypto";
import { apiJson } from "../api/client.js";
import { ISSUER } from "../auth/config.js";
import { success, fields, warn } from "../ui/format.js";
import { link } from "./link.js";

const CONFIG_FILE = ".aid.json";

export async function sync(args = []) {
  const [appId, ...extra] = args;
  if (extra.length) throw new Error("Usage: aid sync [<app_id>]");
  const configPath = path.join(process.cwd(), CONFIG_FILE);

  if (appId) {
    // Link handles identity validation and atomically replaces the local link.
    // It also avoids carrying a client over when the application changes.
    await link([appId, "--force"]);
    return;
  }

  const config = await readConfig(configPath);
  if (!config) throw new Error("No .aid.json found. Run `aid link <app_id>` first.");
  if (config.issuer !== ISSUER) {
    throw new Error("The linked project uses a different issuer. Refusing to sync it against the current account.");
  }
  if (typeof config.app_id !== "string" || !config.app_id.trim()) {
    throw new Error("The local .aid.json has no valid app_id. Run `aid link <app_id> --force`.");
  }

  const app = await apiJson(`/apps/${encodeURIComponent(config.app_id)}`);
  if (!app || typeof app !== "object" || Array.isArray(app) || typeof app.id !== "string" || !app.id) {
    throw new Error("The linked application response was invalid. Local configuration was not changed.");
  }
  if (app.id !== config.app_id) {
    throw new Error("The API returned a different application ID. Local configuration was not changed.");
  }

  let client = null;
  if (config.client_id) {
    if (typeof config.client_id !== "string" || !config.client_id.trim()) {
      throw new Error("The local .aid.json contains an invalid client_id. Local configuration was not changed.");
    }
    client = await apiJson(`/clients/${encodeURIComponent(config.client_id)}`);
    if (!client || typeof client !== "object" || Array.isArray(client) ||
        (client.client_id && client.client_id !== config.client_id)) {
      throw new Error("The linked OAuth client response was invalid. Local configuration was not changed.");
    }
  }

  // Remote application/client metadata is canonical; unrelated project settings
  // remain untouched. Nothing is written until every remote read has succeeded.
  const next = {
    ...config,
    app_id: app.id,
    app_name: typeof app.name === "string" ? app.name : config.app_name,
    app_description: typeof app.description === "string" ? app.description : config.app_description,
    ...(client && Array.isArray(client.redirect_uris) && client.redirect_uris.length
      ? { redirect_uri: client.redirect_uris[0] }
      : {}),
    ...(client && Array.isArray(client.scopes)
      ? { scopes: [...new Set(client.scopes.filter((scope) => typeof scope === "string" && scope.trim()))] }
      : {}),
    synced_at: new Date().toISOString()
  };

  await writeConfig(configPath, next);

  success(`Synced ${app.name || app.id}.`);
  fields([
    ["Application ID", app.id],
    ["Application", app.name || app.id],
    ...(client ? [["Client ID", client.client_id || config.client_id]] : []),
    ["Remote metadata", "up to date"],
    ["Project settings", "preserved"],
    ["Last synced", next.synced_at],
    ["Config", CONFIG_FILE]
  ]);
}

async function readConfig(file) {
  let raw;
  try {
    raw = await fs.readFile(file, "utf8");
  } catch (err) {
    if (err.code === "ENOENT") return null;
    throw new Error(`Could not read ${CONFIG_FILE}: ${err.message}`);
  }

  let config;
  try {
    config = JSON.parse(raw);
  } catch {
    throw new Error(`${CONFIG_FILE} contains invalid JSON. Fix it before syncing; the file was left untouched.`);
  }
  if (!config || typeof config !== "object" || Array.isArray(config)) {
    throw new Error(`${CONFIG_FILE} must contain a JSON object. The file was left untouched.`);
  }
  return config;
}

async function writeConfig(file, config) {
  const tmp = `${file}.${process.pid}.${randomUUID()}.tmp`;
  try {
    await fs.writeFile(tmp, JSON.stringify(config, null, 2) + "\n", { mode: 0o600 });
    await fs.chmod(tmp, 0o600);
    await fs.rename(tmp, file);
  } catch (err) {
    await fs.rm(tmp, { force: true }).catch(() => {});
    throw new Error(`Could not update ${CONFIG_FILE}: ${err.message}`);
  }
}
