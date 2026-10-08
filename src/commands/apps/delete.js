import { apiFetch } from "../../api/client.js";
import { success } from "../../ui/format.js";

export async function del(args) {
  const ids = args.filter(Boolean);

  if (!ids.length) throw new Error("Usage: aid apps delete <id> [<id> ...]");

  for (const id of ids) {
    const response = await apiFetch(`/apps/${encodeURIComponent(id)}`, { method: "DELETE" });

    if (response.status === 404) throw new Error(`No app found with ID ${id}`);
    if (!response.ok) {
      const text = await response.text();
      throw new Error(`Delete failed (${response.status}) for ${id}: ${text}`);
    }

    success(`Deleted ${id}`);
  }
}
