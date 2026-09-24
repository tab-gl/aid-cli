import { apiFetch } from "../../api/client.js";
import { success } from "../../ui/format.js";

export async function del(args) {
  const [id] = args;
  if (!id) throw new Error("Usage: aid clients delete <client_id>");

  const response = await apiFetch(`/clients/${encodeURIComponent(id)}`, { method: "DELETE" });
  if (response.status === 404) throw new Error(`No client found with ID ${id}`);
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Delete failed (${response.status}): ${text}`);
  }

  success(`Deleted ${id}`);
}
