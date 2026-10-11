import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

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
