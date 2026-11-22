import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";

const moduleUrl = new URL("./tokens.js", import.meta.url).href;

function loadInChild(configHome) {
  return spawnSync(process.execPath, ["--input-type=module", "-e", `
    import { loadTokens } from ${JSON.stringify(moduleUrl)};
    try {
      const tokens = await loadTokens();
      console.log(JSON.stringify(tokens));
    } catch (error) {
      console.error(error.message);
      process.exitCode = 1;
    }
  `], {
    encoding: "utf8",
    env: { ...process.env, XDG_CONFIG_HOME: configHome }
  });
}


async function waitForFile(file, timeoutMs = 5_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      return await fs.readFile(file, "utf8");
    } catch (err) {
      if (err.code !== "ENOENT") throw err;
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  }
  throw new Error(`Timed out waiting for ${path.basename(file)}.`);
}

function startLockWorker(configHome, readyFile, releaseFile) {
  const script = `
    import fs from "node:fs/promises";
    import { withTokenLock } from ${JSON.stringify(moduleUrl)};
    const readyFile = ${JSON.stringify(readyFile)};
    const releaseFile = ${JSON.stringify(releaseFile)};
    await withTokenLock(async () => {
      await fs.writeFile(readyFile, String(process.pid));
      const deadline = Date.now() + 10_000;
      while (Date.now() < deadline) {
        try {
          await fs.access(releaseFile);
          return;
        } catch (err) {
          if (err.code !== "ENOENT") throw err;
          await new Promise((resolve) => setTimeout(resolve, 10));
        }
      }
      throw new Error("Timed out waiting for test release.");
    });
  `;
  const child = spawn(process.execPath, ["--input-type=module", "-e", script], {
    env: { ...process.env, XDG_CONFIG_HOME: configHome },
    stdio: ["ignore", "ignore", "pipe"]
  });
  let stderr = "";
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk) => { stderr += chunk; });
  const done = new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("close", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`lock worker exited (${code}): ${stderr}`));
    });
  });
  return { child, done };
}

test("re-hardens existing session directory and file permissions", async (t) => {
  const configHome = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-token-mode-"));
  t.after(() => fs.rm(configHome, { recursive: true, force: true }));
  const dir = path.join(configHome, "aidc");
  const file = path.join(dir, "tokens.json");
  await fs.mkdir(dir, { recursive: true, mode: 0o777 });
  await fs.writeFile(file, JSON.stringify({
    access_token: "mode-test-token",
    expires_at: Date.now() + 60_000
  }), { mode: 0o644 });
  if (process.platform !== "win32") await fs.chmod(dir, 0o755);

  const result = loadInChild(configHome);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).access_token, "mode-test-token");
  if (process.platform !== "win32") {
    assert.equal((await fs.stat(dir)).mode & 0o777, 0o700);
    assert.equal((await fs.stat(file)).mode & 0o777, 0o600);
  }
});

test("rejects symlink-based session files", { skip: process.platform === "win32" }, async (t) => {
  const configHome = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-token-link-"));
  t.after(() => fs.rm(configHome, { recursive: true, force: true }));
  const dir = path.join(configHome, "aidc");
  await fs.mkdir(dir, { recursive: true });
  const target = path.join(configHome, "external-session.json");
  await fs.writeFile(target, JSON.stringify({
    access_token: "must-not-be-read",
    expires_at: Date.now() + 60_000
  }), { mode: 0o600 });
  await fs.symlink(target, path.join(dir, "tokens.json"));

  const result = loadInChild(configHome);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /invalid or unsafe/);
});



test("a lock holder does not remove a replacement lock after stale recovery", { skip: process.platform === "win32" }, async (t) => {
  const configHome = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-lock-owner-"));
  const dir = path.join(configHome, "aidc");
  const lockFile = path.join(dir, "tokens.json.lock");
  await fs.mkdir(dir, { recursive: true });

  const releaseA = path.join(configHome, "release-a");
  const releaseB = path.join(configHome, "release-b");
  const readyA = path.join(configHome, "ready-a");
  const readyB = path.join(configHome, "ready-b");
  const workers = [];
  t.after(async () => {
    await Promise.all([releaseA, releaseB].map((file) => fs.writeFile(file, "release").catch(() => {})));
    await Promise.all(workers.map(async ({ child, done }) => {
      if (child.exitCode === null) child.kill();
      await done.catch(() => {});
    }));
    await fs.rm(configHome, { recursive: true, force: true });
  });

  const workerA = startLockWorker(configHome, readyA, releaseA);
  workers.push(workerA);
  const pidA = Number(await waitForFile(readyA));
  const original = JSON.parse(await fs.readFile(lockFile, "utf8"));
  // The parent process simulates a recycled, still-live PID recorded in an old lock.
  await fs.writeFile(lockFile, JSON.stringify({ ...original, pid: process.pid }));
  const staleTime = new Date(Date.now() - 120_000);
  await fs.utimes(lockFile, staleTime, staleTime);

  const workerB = startLockWorker(configHome, readyB, releaseB);
  workers.push(workerB);
  const pidB = Number(await waitForFile(readyB));
  assert.notEqual(pidB, pidA);

  await fs.writeFile(releaseA, "release");
  await workerA.done;
  const survivingLock = JSON.parse(await fs.readFile(lockFile, "utf8"));
  assert.equal(survivingLock.pid, pidB, "the first owner must not unlink the replacement owner's lock");

  await fs.writeFile(releaseB, "release");
  await workerB.done;
  await assert.rejects(fs.access(lockFile), { code: "ENOENT" });
});

test("rejects session tokens with a missing or null expiration", async (t) => {
  for (const expiresAt of [undefined, null]) {
    const configHome = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-token-expiry-"));
    t.after(() => fs.rm(configHome, { recursive: true, force: true }));
    const dir = path.join(configHome, "aidc");
    await fs.mkdir(dir, { recursive: true });
    const tokens = { access_token: "expiry-test-token" };
    if (expiresAt !== undefined) tokens.expires_at = expiresAt;
    await fs.writeFile(path.join(dir, "tokens.json"), JSON.stringify(tokens), { mode: 0o600 });

    const result = loadInChild(configHome);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /invalid or unsafe/);
  }
});

test("stale-token 401 cleanup preserves a newer saved session", async (t) => {
  const configHome = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-token-race-"));
  t.after(() => fs.rm(configHome, { recursive: true, force: true }));
  const dir = path.join(configHome, "aidc");
  await fs.mkdir(dir, { recursive: true });
  const file = path.join(dir, "tokens.json");
  await fs.writeFile(file, JSON.stringify({
    access_token: "new-access-token",
    refresh_token: "new-refresh-token",
    expires_at: Date.now() + 3_600_000
  }), { mode: 0o600 });

  const script = `
    import { clearTokensIfAccessToken, loadTokens } from ${JSON.stringify(moduleUrl)};
    const cleared = await clearTokensIfAccessToken("old-access-token");
    console.log(JSON.stringify({ cleared, tokens: await loadTokens() }));
  `;
  const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
    encoding: "utf8",
    env: { ...process.env, XDG_CONFIG_HOME: configHome }
  });

  assert.equal(result.status, 0, result.stderr);
  const output = JSON.parse(result.stdout);
  assert.equal(output.cleared, false);
  assert.equal(output.tokens.access_token, "new-access-token");
});
