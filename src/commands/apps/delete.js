import { apiFetch } from "../../api/client.js";

export async function del(args) {
  const [id] = args;

  if (!id) {
    throw new Error(
      "Usage: aidc apps delete <id>"
    );
  }

  const response = await apiFetch(
    `/apps/${encodeURIComponent(id)}`,
    { method: "DELETE" }
  );

  /*
   * apiFetch throws on 401 and 5xx-adjacent
   * codes but not on 404 — we handle that
   * here so the user gets a clean message.
   */
  if (response.status === 404) {
    throw new Error(
      `No app found with ID ${id}`
    );
  }

  if (!response.ok) {
    const text = await response.text();

    throw new Error(
      `Delete failed (${response.status}): ${text}`
    );
  }

  console.log(`\n✓ Deleted ${id}\n`);
}
