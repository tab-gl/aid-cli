import { apiFetch } from "../../api/client.js";

export async function del(args) {
  const [id] = args;

  if (!id) {
    throw new Error("Usage: aidc clients delete <client_id>");
  }

  const response = await apiFetch(
    `/clients/${encodeURIComponent(id)}`,
    { method: "DELETE" }
  );

  if (response.status === 404) {
    throw new Error(`No client found with ID ${id}`);
  }

  if (!response.ok) {
    const text = await response.text();

    throw new Error(
      `Delete failed (${response.status}): ${text}`
    );
  }

  console.log(`\n✓ Deleted ${id}\n`);
}
