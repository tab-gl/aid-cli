import { apiJson } from "../api/client.js";
import { fields } from "../ui/format.js";

export async function whoami() {
  const user = await apiJson("/me");

  const name =
    user.name ||
    user.preferred_username ||
    user.email ||
    user.sub;

  fields([
    ["Signed in as", name],
    ["Email", user.email],
    ["Subject", user.sub]
  ]);
}
