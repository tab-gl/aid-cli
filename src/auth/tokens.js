import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";

import { REFRESH_LEEWAY_MS } from "./config.js";

const DIR =
  process.platform === "win32"
    ? path.join(process.env.APPDATA || os.homedir(), "aidc")
    : path.join(
        process.env.XDG_CONFIG_HOME ||
          path.join(os.homedir(), ".config"),
        "aidc"
      );

const FILE = path.join(DIR, "tokens.json");

export async function saveTokens(tokens) {
  await fs.mkdir(DIR, { recursive: true, mode: 0o700 });

  const tmp = `${FILE}.${process.pid}.tmp`;

  await fs.writeFile(
    tmp,
    JSON.stringify(tokens, null, 2),
    { mode: 0o600 }
  );

  await fs.rename(tmp, FILE);
}

export async function loadTokens() {
  try {
    const raw = await fs.readFile(FILE, "utf8");
    return JSON.parse(raw);
  } catch (err) {
    if (err.code === "ENOENT") return null;
    throw err;
  }
}

export async function clearTokens() {
  try {
    await fs.unlink(FILE);
  } catch (err) {
    if (err.code !== "ENOENT") throw err;
  }
}

export function isExpiringSoon(tokens) {
  if (!tokens?.expires_at) return true;
  return Date.now() + REFRESH_LEEWAY_MS >= tokens.expires_at;
}
