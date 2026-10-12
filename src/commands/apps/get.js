import { apiJson } from "../../api/client.js";
import { fields, jsonOutput } from "../../ui/format.js";

export async function get(args = []) {
  const [id, ...rest] = args;
  const json = rest.length === 1 && rest[0] === "--json";
  if (!id || (rest.length && !json)) {
    throw new Error("Usage: aid apps get <id> [--json]");
  }

  const app = await apiJson(`/apps/${encodeURIComponent(id)}`);
  if (!app || typeof app !== "object" || Array.isArray(app)) {
    throw new Error("The API returned an unexpected application response.");
  }

  if (json) {
    jsonOutput({
      ...(app.id !== undefined ? { id: app.id } : {}),
      ...(app.name !== undefined ? { name: app.name } : {}),
      ...(app.description !== undefined ? { description: app.description } : {}),
      ...(app.created_at !== undefined ? { created_at: app.created_at } : {})
    });
    return;
  }

  fields([
    ["ID", app.id],
    ["Name", app.name],
    ["Description", app.description],
    ["Created", app.created_at]
  ]);
}
