import fs from "node:fs/promises";
import path from "node:path";
import { randomUUID } from "node:crypto";
import { parseArgs } from "node:util";
import { apiJson } from "../api/client.js";
import { ISSUER } from "../auth/config.js";
import { success, fields } from "../ui/format.js";
import { link } from "./link.js";

const CONFIG_FILE = ".aid.json";

export async function sync(args = []) {
  let parsed;
  try {
    parsed = parseArgs({
      args,
      options: { "dry-run": { type: "boolean" } },
      strict: true,
      allowPositionals: true
    });
  } catch {
    throw new Error("Usage: aid sync [<app_id>] [--dry-run]");
  }
  const [appId, ...extra] = parsed.positionals;
  const dryRun = parsed.values["dry-run"] === true;
  if (extra.length || (appId && dryRun)) throw new Error("Usage: aid sync [<app_id>] [--dry-run]");

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

  // Build the target from remote fields, changing only metadata the server
  // actually supplied. Unrelated local project settings remain untouched.
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

  const changes = diffMetadata(config, next);
  console.log("\\nSYNC PLAN");
  if (!changes.length) {
    console.log("  No metadata changes detected. Local configuration is already up to date.");
    fields([
      ["Application ID", app.id],
      ["Application", app.name || app.id],
      ...(client ? [["Client ID", client.client_id]] : []),
      ["Changes", "none"],
      ["Config", CONFIG_FILE]
    ]);
    return;
  }

  for (const change of changes) {
    console.log(`  ${change.field}: ${change.before} -> ${change.after}`);
  }
  if (dryRun) {
    console.log("\\nDry run complete. No files were changed.");
    return;
  }

  // Avoid overwriting edits made while remote requests were in flight.
  const currentRaw = await fs.readFile(configPath, "utf8").catch((err) => {
    if (err.code === "ENOENT") return null;
    throw new Error(`Could not re-check ${CONFIG_FILE} before writing: ${err.message}`);
  });
  if (currentRaw !== snapshot.raw) {
    throw new Error(`${CONFIG_FILE} changed while sync was running. No local changes were written; rerun the sync.`);
  }

  next.synced_at = new Date().toISOString();
  await writeConfig(configPath, next);

  success(`Synced ${app.name || app.id}.`);
  fields([
    ["Application ID", app.id],
    ["Application", app.name || app.id],
    ...(client ? [["Client ID", client.client_id]] : []),
    ["Changed fields", String(changes.length)],
    ["Project settings", "preserved"],
    ["Last synced", next.synced_at],
    ["Config", CONFIG_FILE]
  ]);
}

function diffMetadata(current, next) {
  const keys = [
    ["app_name", "Application name"],
    ["app_description", "Application description"],
    ["redirect_uri", "Redirect URI"],
    ["scopes", "Scopes"]
  ];
  const display = (value, exists) => {
    if (!exists) return "(unset)";
    if (Array.isArray(value)) return JSON.stringify(value);
    if (typeof value === "string") return JSON.stringify(value);
    return JSON.stringify(value);
  };
  return keys.flatMap(([key, label]) => {
    const beforeExists = Object.hasOwn(current, key);
    const afterExists = Object.hasOwn(next, key);
    const before = current[key];
    const after = next[key];
    if (beforeExists === afterExists && JSON.stringify(before) === JSON.stringify(after)) return [];
    return [{
      field: label,
      before: display(before, beforeExists),
      after: display(after, afterExists)
    }];
  });
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
