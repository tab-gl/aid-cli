import { apiJson } from "../../api/client.js";
import { fields } from "../../ui/format.js";

export async function get(args) {
  const [id] = args;

  if (!id) {
    throw new Error("Usage: aidc apps get <id>");
  }

  const app = await apiJson(`/apps/${encodeURIComponent(id)}`);

  fields([
    ["ID", app.id],
    ["Name", app.name],
    ["Description", app.description],
    ["Created", app.created_at]
  ]);
}
