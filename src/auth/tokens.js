import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";

import { REFRESH_LEEWAY_MS } from "./config.js";

const DIR =
  process.platform === "win32"
    ? path.join(process.env.APPDATA || os.homedir(), "aidc")
    : path.join(
        process.env.XDG_CONFIG_HOME || path.join(os.homedir(), ".config"),
        "aidc"
      );

const FILE = path.join(DIR, "tokens.json");

export async function saveTokens(tokens) {
  const normalized = validateTokens(tokens);
  await fs.mkdir(DIR, { recursive: true, mode: 0o700 });

  const tmp = `${FILE}.${process.pid}.tmp`;
  await fs.writeFile(tmp, JSON.stringify(normalized, null, 2), { mode: 0o600 });
  await fs.rename(tmp, FILE);
}

export async function loadTokens() {
  try {
    const raw = await fs.readFile(FILE, "utf8");
    return validateTokens(JSON.parse(raw));
  } catch (err) {
    if (err.code === "ENOENT") return null;
    if (err instanceof SyntaxError || (err instanceof Error && err.message === "INVALID_TOKEN_DATA")) {
      throw new Error("Stored Ace ID session is invalid. Run `aid login` again.");
    }
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

function validateTokens(tokens) {
  if (!tokens || typeof tokens !== "object" || Array.isArray(tokens)) {
    throw new Error("INVALID_TOKEN_DATA");
  }

  if (typeof tokens.access_token !== "string" || !tokens.access_token.trim()) {
    throw new Error("INVALID_TOKEN_DATA");
  }

  if (tokens.refresh_token !== undefined && typeof tokens.refresh_token !== "string") {
    throw new Error("INVALID_TOKEN_DATA");
  }

  if (tokens.id_token !== undefined && typeof tokens.id_token !== "string") {
    throw new Error("INVALID_TOKEN_DATA");
  }

  if (
    tokens.expires_at !== undefined &&
    (!Number.isFinite(tokens.expires_at) || tokens.expires_at <= 0)
  ) {
    throw new Error("INVALID_TOKEN_DATA");
  }

  return {
    ...tokens,
    access_token: tokens.access_token.trim()
  };
}
