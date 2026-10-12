import { apiJson } from "../../api/client.js";
import { fields, jsonOutput } from "../../ui/format.js";

export async function get(args = []) {
  const [id, ...rest] = args;
  const json = rest.length === 1 && rest[0] === "--json";
  if (!id || (rest.length && !json)) {
    throw new Error("Usage: aid clients get <client_id> [--json]");
  }

  const client = await apiJson(`/clients/${encodeURIComponent(id)}`);
  if (!client || typeof client !== "object" || Array.isArray(client)) {
    throw new Error("The API returned an unexpected OAuth client response.");
  }

  if (json) {
    jsonOutput({
      ...(client.client_id !== undefined ? { client_id: client.client_id } : {}),
      ...(client.name !== undefined ? { name: client.name } : {}),
      ...(client.public !== undefined ? { public: client.public } : {}),
      ...(client.app_id !== undefined ? { app_id: client.app_id } : {}),
      ...(client.redirect_uris !== undefined ? { redirect_uris: client.redirect_uris } : {}),
      ...(client.scopes !== undefined ? { scopes: client.scopes } : {}),
      ...(client.created_at !== undefined ? { created_at: client.created_at } : {})
    });
    return;
  }

  fields([
    ["Client ID", client.client_id],
    ["Name", client.name],
    ["Type", client.public ? "public" : "confidential"],
    ["Redirect URIs", (client.redirect_uris || []).join("\n")],
    ["Scopes", (client.scopes || []).join(" ")],
    ["Created", client.created_at]
  ]);
}
