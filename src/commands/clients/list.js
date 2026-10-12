import fs from "node:fs/promises";
import path from "node:path";
import { apiJson } from "../../api/client.js";
import { table, jsonOutput } from "../../ui/format.js";
import { filterResources, parseListOptions } from "../list-options.js";

export async function list(args = []) {
  const options = parseListOptions(args, "Usage: aid clients list [--search <text>] [--limit <n>] [--json]");
  const [response, appResponse, localConfig] = await Promise.all([
    apiJson("/clients"),
    apiJson("/apps"),
    readLocalConfig()
  ]);
  if (!Array.isArray(response) || response.some((client) => !client || typeof client !== "object" || Array.isArray(client))) {
    throw new Error("The API returned an unexpected OAuth client list. Expected an array of client objects.");
  }
  if (!Array.isArray(appResponse) || appResponse.some((app) => !app || typeof app !== "object" || Array.isArray(app))) {
    throw new Error("The API returned an unexpected application list while resolving OAuth client ownership.");
  }

  const appsById = new Map(appResponse.filter((app) => typeof app.id === "string").map((app) => [app.id, app]));
  const clientsWithRelations = response.map((client) => {
    const appId = typeof client.app_id === "string" ? client.app_id : "";
    const owner = appId ? appsById.get(appId) : null;
    const isCurrentClient = localConfig?.client_id === client.client_id;
    const mislinked = Boolean(isCurrentClient && localConfig?.app_id &&
      appId && String(localConfig.app_id) !== appId);
    const orphaned = !appId || !owner;
    const redirectMismatch = Boolean(isCurrentClient &&
      typeof localConfig?.redirect_uri === "string" &&
      Array.isArray(client.redirect_uris) &&
      !client.redirect_uris.includes(localConfig.redirect_uri));
    const status = mislinked
      ? "MISLINKED"
      : orphaned
        ? (appId ? "ORPHANED: app missing" : "ORPHANED: no app")
        : redirectMismatch
          ? "REDIRECT MISMATCH"
          : "OK";
    return {
      ...client,
      _app_name: owner?.name || (appId ? appId : "(unassigned)"),
      _relationship_status: status,
      _redirect_uri_mismatch: redirectMismatch
    };
  });

  const clients = filterResources(
    clientsWithRelations,
    options.search,
    options.limit,
    (client) => [
      client.client_id,
      client.name,
      client.public ? "public" : "confidential",
      client.app_id,
      client._app_name,
      client._relationship_status,
      ...(Array.isArray(client.redirect_uris) ? client.redirect_uris : []),
      ...(Array.isArray(client.scopes) ? client.scopes : [])
    ]
  );

  if (options.json) {
    jsonOutput(clients.map((client) => ({
      client_id: client.client_id,
      ...(client.name !== undefined ? { name: client.name } : {}),
      ...(client.public !== undefined ? { public: client.public } : {}),
      ...(client.app_id !== undefined ? { app_id: client.app_id } : {}),
      application_name: client._app_name,
      relationship_status: client._relationship_status,
      redirect_uri_mismatch: client._redirect_uri_mismatch,
      ...(client.redirect_uris !== undefined ? { redirect_uris: client.redirect_uris } : {}),
      ...(client.scopes !== undefined ? { scopes: client.scopes } : {}),
      ...(client.created_at !== undefined ? { created_at: client.created_at } : {})
    })));
    return;
  }

  table(clients, [
    { header: "CLIENT ID", value: (client) => client.client_id },
    { header: "NAME", value: (client) => client.name },
    { header: "TYPE", value: (client) => (client.public ? "public" : "confidential") },
    { header: "APPLICATION", value: (client) => client._app_name },
    { header: "RELATION", value: (client) => client._relationship_status }
  ]);
}

async function readLocalConfig() {
  try {
    const raw = await fs.readFile(path.join(process.cwd(), ".aid.json"), "utf8");
    const config = JSON.parse(raw);
    return config && typeof config === "object" && !Array.isArray(config) ? config : null;
  } catch (err) {
    if (err.code === "ENOENT" || err instanceof SyntaxError) return null;
    throw new Error(`Could not inspect local .aid.json: ${err.message}`);
  }
}
