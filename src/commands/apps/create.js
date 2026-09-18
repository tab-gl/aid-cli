import { parseArgs } from "node:util";
import { apiJson } from "../../api/client.js";
import { success, fields } from "../../ui/format.js";

export async function create(args) {
  let parsed;

  try {
    parsed = parseArgs({
      args,
      options: {
        name:        { type: "string" },
        description: { type: "string" }
      }
    });
  } catch {
    throw new Error(
      "Usage: aidc apps create --name <name> [--description <text>]"
    );
  }

  const { values } = parsed;

  if (!values.name) {
    throw new Error("--name is required.");
  }

  const app = await apiJson("/apps", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      name: values.name,
      description: values.description
    })
  });

  success(`Created ${app.name}`);

  fields([
    ["ID", app.id]
  ]);
}
