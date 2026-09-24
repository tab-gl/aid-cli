import { parseArgs } from "node:util";
import { apiJson } from "../../api/client.js";
import { success } from "../../ui/format.js";

export async function update(args) {
  const [id, ...rest] = args;
  if (!id) throw new Error("Usage: aid apps update <id> [--name <name>] [--description <text>]");

  let parsed;
  try {
    parsed = parseArgs({ args: rest, options: {
      name: { type: "string" },
      description: { type: "string" }
    }});
  } catch {
    throw new Error("Usage: aid apps update <id> [--name <name>] [--description <text>]");
  }

  const body = {};
  if (parsed.values.name !== undefined) body.name = parsed.values.name;
  if (parsed.values.description !== undefined) body.description = parsed.values.description;
  if (Object.keys(body).length === 0) throw new Error("Nothing to update. Pass --name and/or --description.");

  const app = await apiJson(`/apps/${encodeURIComponent(id)}`, {
    method: "PATCH",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body)
  });

  success(`Updated ${app.name}`);
}
