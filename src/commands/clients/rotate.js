import { apiJson } from "../../api/client.js";
import { success, fields, warn } from "../../ui/format.js";

export async function rotate(args) {
  const [id] = args;
  if (!id) throw new Error("Usage: aid clients rotate-secret <client_id>");

  const result = await apiJson(`/clients/${encodeURIComponent(id)}/rotate-secret`, { method: "POST" });

  success(`Rotated secret for ${result.client_id}`);
  fields([["Client Secret", result.client_secret]]);
  warn("Save this secret now. It will not be shown again.");
  warn("The previous secret is immediately invalid.");
  console.log("");
}
