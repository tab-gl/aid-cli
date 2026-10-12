import fs from "node:fs/promises";
import path from "node:path";
import { randomUUID } from "node:crypto";
import { apiJson } from "../api/client.js";
import { ISSUER } from "../auth/config.js";
import { success, fields } from "../ui/format.js";
import { link } from "./link.js";

const CONFIG_FILE = ".aid.json";

export async function sync(args = []) {
  const [appId, ...extra] = args;
  if (extra.length) throw new Error("Usage: aid sync [<app_id>]");
  const configPath = path.join(process.cwd(), CONFIG_FILE);

  if (appId) {
    // Switching the app intentionally drops metadata belonging to the old app.
    await link([appId, "--force"]);
    return;
  }

  const snapshot = await readConfig(configPath);
  if (!snapshot) throw new Error("No .aid.json found. Run `aid link <app_id>` first.");
  const { config } = snapshot;
  if (config.issuer !== ISSUER) {
    throw new Error("The linked project uses a different issuer. Refusing to sync it against the current account.");
  }
  if (typeof config.app_id !== "string" || !config.app_id.trim()) {
    throw new Error("The local .aid.json has no valid app_id. Run `aid link <app_id> --force`.");
  }

  let clientPromise = null;
  if (config.client_id !== undefined && config.client_id !== null) {
    if (typeof config.client_id !== "string" || !config.client_id.trim()) {
      throw new Error("The local .aid.json contains an invalid client_id. Local configuration was not changed.");
    }
    clientPromise = apiJson(`/clients/${encodeURIComponent(config.client_id)}`);
  }

  // Both reads are independent. Fetch them together to avoid paying two full
  // network round trips, but validate both responses before touching the file.
  const [app, client] = await Promise.all([
    apiJson(`/apps/${encodeURIComponent(config.app_id)}`),
    clientPromise || Promise.resolve(null)
  ]);

  if (!app || typeof app !== "object" || Array.isArray(app) || typeof app.id !== "string" || !app.id) {
    throw new Error("The linked application response was invalid. Local configuration was not changed.");
  }
  if (app.id !== config.app_id) {
    throw new Error("The API returned a different application ID. Local configuration was not changed.");
  }
  for (const key of ["name", "description"]) {
    if (Object.hasOwn(app, key) && app[key] !== null && typeof app[key] !== "string") {
      throw new Error(`The linked application's ${key} field was invalid. Local configuration was not changed.`);
    }
  }

  if (config.client_id !== undefined && config.client_id !== null) {
    if (!client || typeof client !== "object" || Array.isArray(client) ||
        client.client_id !== config.client_id ||
        (client.app_id !== undefined && client.app_id !== null && String(client.app_id) !== config.app_id)) {
      throw new Error("The linked OAuth client response was invalid or belongs to another application. Local configuration was not changed.");
    }
    if (Object.hasOwn(client, "redirect_uris") &&
        (!Array.isArray(client.redirect_uris) || client.redirect_uris.some((uri) => typeof uri !== "string"))) {
      throw new Error("The linked OAuth client returned invalid redirect URIs. Local configuration was not changed.");
    }
    if (Object.hasOwn(client, "scopes") &&
        (!Array.isArray(client.scopes) || client.scopes.some((scope) => typeof scope !== "string"))) {
      throw new Error("The linked OAuth client returned invalid scopes. Local configuration was not changed.");
    }
  }

  // Apply only validated remote fields. If the server explicitly clears a field,
  // remove stale local metadata; if it omits a field entirely, leave it alone.
  const next = { ...config, app_id: app.id };
  if (Object.hasOwn(app, "name")) {
    if (typeof app.name === "string" && app.name.trim()) next.app_name = app.name;
    else delete next.app_name;
  }
  if (Object.hasOwn(app, "description")) {
    if (typeof app.description === "string" && app.description.trim()) next.app_description = app.description;
    else delete next.app_description;
  }
  if (client && Object.hasOwn(client, "redirect_uris")) {
    if (client.redirect_uris.length) next.redirect_uri = client.redirect_uris[0];
    else delete next.redirect_uri;
  }
  if (client && Object.hasOwn(client, "scopes")) {
    next.scopes = [...new Set(client.scopes.map((scope) => scope.trim()).filter(Boolean))];
  }
  next.synced_at = new Date().toISOString();

  // Avoid overwriting edits made while remote requests were in flight.
  const currentRaw = await fs.readFile(configPath, "utf8").catch((err) => {
    if (err.code === "ENOENT") return null;
    throw new Error(`Could not re-check ${CONFIG_FILE} before writing: ${err.message}`);
  });
  if (currentRaw !== snapshot.raw) {
    throw new Error(`${CONFIG_FILE} changed while sync was running. No local changes were written; rerun the sync.`);
  }

  await writeConfig(configPath, next);

  success(`Synced ${app.name || app.id}.`);
  fields([
    ["Application ID", app.id],
    ["Application", app.name || app.id],
    ...(client ? [["Client ID", client.client_id]] : []),
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
  return { config, raw };
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
