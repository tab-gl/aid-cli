import { parseArgs } from "node:util";
import { apiJson } from "../../api/client.js";
import { success, fields, warn } from "../../ui/format.js";

export async function create(args) {
  let parsed;

  try {
    parsed = parseArgs({
      args,
      options: {
        name:     { type: "string" },
        redirect: { type: "string", multiple: true },
        scope:    { type: "string", multiple: true },
        id:       { type: "string" },
        public:   { type: "boolean" }
      }
    });
  } catch {
    throw new Error(
      "Usage: aid clients create --name <name> --redirect <uri> " +
      "[--redirect <uri>...] [--scope <s>...] [--id <id>] [--public]"
    );
  }

  const { values } = parsed;

  if (!values.name) {
    throw new Error("--name is required.");
  }

  if (!values.redirect || values.redirect.length === 0) {
    throw new Error("At least one --redirect is required.");
  }

  const body = {
    name: values.name,
    redirect_uris: values.redirect
  };

  if (values.scope?.length) body.scopes = values.scope;
  if (values.id)             body.client_id = values.id;
  if (values.public)         body.public = true;

  const result = await apiJson("/clients", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body)
  });

  success(`Created ${result.name}`);

  fields([
    ["Client ID", result.client_id],
    ["Type", result.public ? "public" : "confidential"]
  ]);

  if (result.client_secret) {
    fields([
      ["Client Secret", result.client_secret]
    ]);

    warn(
      "Save this secret now. It will not be shown again."
    );
    console.log("");
  }
}
