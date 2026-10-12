import { apiJson } from "../../api/client.js";
import { table, jsonOutput } from "../../ui/format.js";
import { filterResources, parseListOptions } from "../list-options.js";

export async function list(args = []) {
  const options = parseListOptions(args, "Usage: aid clients list [--search <text>] [--limit <n>] [--json]");
  const response = await apiJson("/clients");
  if (!Array.isArray(response)) {
    throw new Error("The API returned an unexpected OAuth client list. Expected an array.");
  }

  const clients = filterResources(
    response,
    options.search,
    options.limit,
    (client) => [
      client.client_id,
      client.name,
      client.public ? "public" : "confidential",
      client.app_id,
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
      ...(client.redirect_uris !== undefined ? { redirect_uris: client.redirect_uris } : {}),
      ...(client.scopes !== undefined ? { scopes: client.scopes } : {}),
      ...(client.created_at !== undefined ? { created_at: client.created_at } : {})
    })));
    return;
  }

  table(clients, [
    { header: "CLIENT ID", value: (client) => client.client_id },
    { header: "NAME", value: (client) => client.name },
    { header: "TYPE", value: (client) => (client.public ? "public" : "confidential") }
  ]);
}
