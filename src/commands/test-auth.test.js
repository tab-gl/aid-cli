import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import http from "node:http";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const entry = fileURLToPath(new URL("../../bin/aid.js", import.meta.url));

async function runCli(args, env) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [entry, ...args], {
      env,
      stdio: ["ignore", "pipe", "pipe"]
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.once("error", reject);
    child.once("close", (code) => resolve({ code, stdout, stderr }));
  });
}

test("aid test-auth verifies the stored session against the identity endpoint", async (t) => {
  const server = http.createServer((req, res) => {
    if (req.method !== "GET" || req.url !== "/me") {
      res.writeHead(404).end();
      return;
    }
    if (req.headers.authorization !== "Bearer test-access-token") {
      res.writeHead(401, { "content-type": "application/json" });
      res.end(JSON.stringify({ message: "Unauthorized" }));
      return;
    }
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ sub: "user-test-123", name: "CLI Test User" }));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));

  const configHome = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-test-auth-"));
  t.after(async () => {
    server.closeAllConnections?.();
    await new Promise((resolve) => server.close(resolve));
    await fs.rm(configHome, { recursive: true, force: true });
  });

  const sessionDir = path.join(configHome, "aidc");
  await fs.mkdir(sessionDir, { recursive: true });
  await fs.writeFile(path.join(sessionDir, "tokens.json"), JSON.stringify({
    access_token: "test-access-token",
    expires_at: Date.now() + 60_000
  }), { mode: 0o600 });

  const env = {
    ...process.env,
    AIDC_ISSUER: `http://127.0.0.1:${server.address().port}`,
    AIDC_REFRESH_LEEWAY_MS: "1000",
    XDG_CONFIG_HOME: configHome
  };
  const result = await runCli(["test-auth"], env);

  assert.equal(result.code, 0, result.stderr);
  assert.match(result.stdout, /Authentication test passed/);
  assert.match(result.stdout, /CLI Test User/);
  assert.match(result.stdout, /user-test-123/);
  assert.doesNotMatch(result.stdout, /test-access-token/);

  const jsonResult = await runCli(["test-auth", "--json"], env);
  assert.equal(jsonResult.code, 0, jsonResult.stderr);
  assert.deepEqual(JSON.parse(jsonResult.stdout), {
    status: "ok",
    issuer: env.AIDC_ISSUER,
    client_id: "aidc-cli",
    account: "CLI Test User",
    subject: "user-test-123"
  });
  assert.doesNotMatch(jsonResult.stdout, /test-access-token/);
});

test("aid test-auth explains when no session is stored", async (t) => {
  const configHome = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-test-auth-empty-"));
  t.after(() => fs.rm(configHome, { recursive: true, force: true }));

  const result = await runCli(["test-auth"], {
    ...process.env,
    XDG_CONFIG_HOME: configHome
  });

  assert.equal(result.code, 1);
  assert.match(result.stderr, /Not signed in/);
});
