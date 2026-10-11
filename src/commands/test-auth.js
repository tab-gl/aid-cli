import { ISSUER, CLIENT_ID } from "../auth/config.js";
import { apiJson } from "../api/client.js";
import { fields, success } from "../ui/format.js";

export async function testAuth() {
  const user = await apiJson("/me");

  if (!user || typeof user !== "object" || Array.isArray(user)) {
    throw new Error("Authentication check failed: the identity endpoint returned an unexpected response.");
  }

  const account = user.name || user.preferred_username || user.email || user.sub;
  if (typeof account !== "string" || !account.trim()) {
    throw new Error("Authentication check failed: the identity endpoint did not return a recognizable account.");
  }

  success("Authentication test passed. The stored session can access the Ace ID API.");
  fields([
    ["Issuer", ISSUER],
    ["Client ID", CLIENT_ID],
    ["Account", account],
    ["Subject", user.sub]
  ]);
}
