import { ISSUER, CLIENT_ID } from "../auth/config.js";
import { apiJson } from "../api/client.js";
import { fields, success, jsonOutput } from "../ui/format.js";

export async function testAuth(args = []) {
  const json = args.length === 1 && args[0] === "--json";
  if (args.length && !json) throw new Error("Usage: aid test-auth [--json]");

  const user = await apiJson("/me");
  if (!user || typeof user !== "object" || Array.isArray(user)) {
    throw new Error("Authentication check failed: the identity endpoint returned an unexpected response.");
  }

  const account = user.name || user.preferred_username || user.email || user.sub;
  if (typeof account !== "string" || !account.trim()) {
    throw new Error("Authentication check failed: the identity endpoint did not return a recognizable account.");
  }

  const result = {
    status: "ok",
    issuer: ISSUER,
    client_id: CLIENT_ID,
    account,
    ...(user.sub !== undefined ? { subject: user.sub } : {})
  };

  if (json) {
    jsonOutput(result);
    return;
  }

  success("Authentication test passed. The stored session can access the Ace ID API.");
  fields([
    ["Issuer", result.issuer],
    ["Client ID", result.client_id],
    ["Account", result.account],
    ["Subject", result.subject]
  ]);
}
