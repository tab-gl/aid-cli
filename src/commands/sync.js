import fs from "node:fs/promises";
import path from "node:path";
import { randomUUID } from "node:crypto";
import { apiJson } from "../api/client.js";
import { success, fields } from "../ui/format.js";
import { link } from "./link.js";

const CONFIG_FILE = ".aid.json";

export async function sync(args = []) {
  const [appId, ...extra] = args;
  if (extra.length) throw new Error("Usage: aid sync [<app_id>]");
  const configPath = path.join(process.cwd(), CONFIG_FILE);

  if (appId) { await link([appId, "--force"]); return; }

  let config;
  try { config = JSON.parse(await fs.readFile(configPath, "utf8")); }
  catch { throw new Error("No .aid.json found. Run `aid link <app_id>` first."); }

  if (!config.app_id) throw new Error("The local .aid.json has no app_id. Run `aid link <app_id> --force`.");

  const app = await apiJson(`/apps/${encodeURIComponent(config.app_id)}`);
  if (!app?.id) throw new Error("The linked app could not be loaded.");

  const next = { ...config, app_id: app.id };
  const tmp = `${configPath}.${process.pid}.${randomUUID()}.tmp`;
  try {
    await fs.writeFile(tmp, JSON.stringify(next, null, 2) + "\n", { mode: 0o600 });
    await fs.chmod(tmp, 0o600);
    await fs.rename(tmp, configPath);
  } catch (err) {
    await fs.rm(tmp, { force: true }).catch(() => {});
    throw err;
  }

  success(`Synced ${app.name || app.id}.`);
  fields([["Application ID", app.id], ["Application", app.name || app.id], ["Config", CONFIG_FILE]]);
}
