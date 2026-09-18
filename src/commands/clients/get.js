import { apiJson } from "../../api/client.js";
import { fields } from "../../ui/format.js";

export async function get(args) {
  const [id] = args;

  if (!id) {
    throw new Error("Usage: aidc clients get <client_id>");
  }

  const c = await apiJson(`/clients/${encodeURIComponent(id)}`);

  fields([
    ["Client ID", c.client_id],
    ["Name", c.name],
    ["Type", c.public ? "public" : "confidential"],
    ["Redirect URIs", (c.redirect_uris || []).join("\n")],
    ["Scopes", (c.scopes || []).join(" ")],
    ["Created", c.created_at]
  ]);
}
