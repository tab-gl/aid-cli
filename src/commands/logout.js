import { clearTokens } from "../auth/tokens.js";
import { success } from "../ui/format.js";

export async function logout() {
  await clearTokens();
  success("Signed out. Local tokens deleted.");
}
