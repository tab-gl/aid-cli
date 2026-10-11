import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { randomUUID } from "node:crypto";

import { REFRESH_LEEWAY_MS, REQUEST_TIMEOUT_MS } from "./config.js";

const DIR =
  process.platform === "win32"
    ? path.join(process.env.APPDATA || os.homedir(), "aidc")
    : path.join(
        process.env.XDG_CONFIG_HOME || path.join(os.homedir(), ".config"),
        "aidc"
      );

const FILE = path.join(DIR, "tokens.json");
const LOCK_FILE = `${FILE}.lock`;
const LOCK_WAIT_MS = Math.max(30_000, REQUEST_TIMEOUT_MS * 3);
const STALE_LOCK_MS = Math.max(60_000, REQUEST_TIMEOUT_MS * 4);

export async function saveTokens(tokens) {
  return withTokenLock(() => saveTokensUnlocked(tokens));
}

export async function saveTokensUnlocked(tokens) {
  const normalized = validateTokens(tokens);
  await fs.mkdir(DIR, { recursive: true, mode: 0o700 });
  await hardenDirectory();

  const tmp = `${FILE}.${process.pid}.${randomUUID()}.tmp`;
  try {
    await fs.writeFile(tmp, JSON.stringify(normalized, null, 2), { mode: 0o600, flag: "wx" });
    await hardenFile(tmp);
    await fs.rename(tmp, FILE);
    await hardenFile(FILE);
  } catch (err) {
    await fs.rm(tmp, { force: true }).catch(() => {});
    throw err;
  }
}

export async function loadTokens() {
  try {
    await hardenDirectory();
    const stat = await fs.lstat(FILE);
    if (!stat.isFile() || stat.isSymbolicLink()) {
      throw new Error("INVALID_TOKEN_DATA");
    }
    await hardenFile(FILE);
    const raw = await fs.readFile(FILE, "utf8");
    return validateTokens(JSON.parse(raw));
  } catch (err) {
    if (err.code === "ENOENT") return null;
    if (err instanceof SyntaxError || (err instanceof Error && err.message === "INVALID_TOKEN_DATA")) {
      throw new Error("Stored Ace ID session is invalid or unsafe. Run `aid login` again.");
    }
    throw err;
  }
}

export async function clearTokens() {
  return withTokenLock(() => clearTokensUnlocked());
}

export async function clearTokensUnlocked() {
  try {
    await fs.unlink(FILE);
  } catch (err) {
    if (err.code !== "ENOENT") throw err;
  }
}

export async function withTokenLock(callback) {
  await fs.mkdir(DIR, { recursive: true, mode: 0o700 });
  await hardenDirectory();
  const startedAt = Date.now();
  let handle;

  while (!handle) {
    let openedByUs = false;
    try {
      handle = await fs.open(LOCK_FILE, "wx", 0o600);
      openedByUs = true;
      await handle.writeFile(JSON.stringify({ pid: process.pid, created_at: Date.now() }));
      await handle.sync();
    } catch (err) {
      if (handle) {
        await handle.close().catch(() => {});
        handle = null;
      }
      if (openedByUs) await fs.unlink(LOCK_FILE).catch(() => {});
      if (err.code !== "EEXIST") throw err;
      await removeStaleLock();
      if (Date.now() - startedAt >= LOCK_WAIT_MS) {
        throw new Error("Timed out waiting for the Ace ID session lock. If no other aid process is running, remove the stale session lock and retry.");
      }
      await delay(50);
    }
  }

  try {
    return await callback();
  } finally {
    await handle.close().catch(() => {});
    await fs.unlink(LOCK_FILE).catch((err) => {
      if (err.code !== "ENOENT") throw err;
    });
  }
}

async function removeStaleLock() {
  try {
    const stat = await fs.stat(LOCK_FILE);
    let owner;
    try {
      owner = JSON.parse(await fs.readFile(LOCK_FILE, "utf8"));
    } catch {
      owner = null;
    }

    if (Number.isInteger(owner?.pid) && owner.pid > 0) {
      try {
        process.kill(owner.pid, 0);
        return;
      } catch (err) {
        if (err.code === "EPERM") return;
        if (err.code !== "ESRCH") return;
      }
    } else if (Date.now() - stat.mtimeMs < STALE_LOCK_MS) {
      return;
    }

    // Recheck the lock identity before unlinking so a newly acquired lock is
    // not removed after another process has already recovered the old one.
    const latest = await fs.stat(LOCK_FILE);
    if (latest.ino === stat.ino && latest.mtimeMs === stat.mtimeMs) {
      await fs.unlink(LOCK_FILE).catch((err) => {
        if (err.code !== "ENOENT") throw err;
      });
    }
  } catch (err) {
    if (err.code !== "ENOENT") throw err;
  }
}

async function hardenDirectory() {
  if (process.platform === "win32") return;
  try {
    await fs.chmod(DIR, 0o700);
  } catch (err) {
    if (err.code !== "ENOENT") throw err;
  }
}

async function hardenFile(file) {
  if (process.platform === "win32") return;
  await fs.chmod(file, 0o600);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
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

  for (const key of ["refresh_token", "id_token", "token_type", "scope"]) {
    if (tokens[key] !== undefined && typeof tokens[key] !== "string") {
      throw new Error("INVALID_TOKEN_DATA");
    }
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
