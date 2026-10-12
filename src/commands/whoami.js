import { apiJson } from "../api/client.js";
import { fields, jsonOutput } from "../ui/format.js";

export async function whoami(args = []) {
  const json = args.length === 1 && args[0] === "--json";
  if (args.length && !json) throw new Error("Usage: aid whoami [--json]");

  const user = await apiJson("/me");
  if (!user || typeof user !== "object" || Array.isArray(user)) {
    throw new Error("The identity endpoint returned an unexpected account response.");
  }

  const name = user.name || user.preferred_username || user.email || user.sub;
  const result = {
    ...(name !== undefined ? { name } : {}),
    ...(user.email !== undefined ? { email: user.email } : {}),
    ...(user.sub !== undefined ? { sub: user.sub } : {})
  };

  if (json) {
    jsonOutput(result);
    return;
  }

  fields([
    ["Signed in as", result.name],
    ["Email", result.email],
    ["Subject", result.sub]
  ]);
}
