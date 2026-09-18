import { apiJson } from "../../api/client.js";

export async function rotate(args) {
  const [id] = args;

  if (!id) {
    throw new Error(
      "Usage: aidc clients rotate-secret <client_id>"
    );
  }

  const result = await apiJson(
    `/clients/${encodeURIComponent(id)}/rotate-secret`,
    { method: "POST" }
  );

  console.log("");
  console.log(`✓ Rotated secret for ${result.client_id}`);
  console.log("");
  console.log(`  client_secret   ${result.client_secret}`);
  console.log("");
  console.log(
    "  Save this now. It will not be shown again."
  );
  console.log(
    "  The previous secret is immediately invalid."
  );
  console.log("");
}
