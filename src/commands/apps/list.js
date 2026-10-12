import { apiJson } from "../../api/client.js";
import { table, jsonOutput } from "../../ui/format.js";
import { filterResources, parseListOptions } from "../list-options.js";

export async function list(args = []) {
  const options = parseListOptions(args, "Usage: aid apps list [--search <text>] [--limit <n>] [--json]");
  const response = await apiJson("/apps");
  if (!Array.isArray(response)) {
    throw new Error("The API returned an unexpected application list. Expected an array.");
  }

  const apps = filterResources(
    response,
    options.search,
    options.limit,
    (app) => [app.id, app.name, app.description]
  );

  if (options.json) {
    jsonOutput(apps.map((app) => ({
      id: app.id,
      ...(app.name !== undefined ? { name: app.name } : {}),
      ...(app.description !== undefined ? { description: app.description } : {}),
      ...(app.created_at !== undefined ? { created_at: app.created_at } : {})
    })));
    return;
  }

  table(apps, [
    { header: "ID", value: (app) => String(app.id) },
    { header: "NAME", value: (app) => app.name || "(unnamed)" }
  ]);
}
