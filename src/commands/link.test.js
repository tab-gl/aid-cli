import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import http from "node:http";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const entry = fileURLToPath(new URL("../../bin/aid.js", import.meta.url));

async function fixture(t) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-link-project-"));
  const configHome = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-link-auth-"));
  const requests = [];
  const server = http.createServer((req, res) => {
    requests.push({ method: req.method, url: req.url });
    if (req.method === "GET" && req.url === "/apps/app-1") {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ id: "app-1", name: "Test app" }));
      return;
    }
    if (req.method === "GET" && req.url === "/clients/client-1") {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ client_id: "client-1", app_id: "app-1", redirect_uris: [], scopes: [] }));
      return;
    }
    if (req.method === "GET" && req.url === "/clients/client-other") {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ client_id: "client-other", app_id: "another-app" }));
      return;
    }
    res.writeHead(404, { "content-type": "application/json" });
    res.end(JSON.stringify({ message: "Not found" }));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(async () => {
    server.closeAllConnections?.();
    await new Promise((resolve) => server.close(resolve));
    await fs.rm(root, { recursive: true, force: true });
    await fs.rm(configHome, { recursive: true, force: true });
  });

  const sessionDir = path.join(configHome, "aidc");
  await fs.mkdir(sessionDir, { recursive: true });
  await fs.writeFile(path.join(sessionDir, "tokens.json"), JSON.stringify({
    access_token: "link-test-token",
    expires_at: Date.now() + 3_600_000
  }), { mode: 0o600 });

  return {
    root,
    requests,
    env: {
      ...process.env,
      AIDC_ISSUER: `http://127.0.0.1:${server.address().port}`,
      XDG_CONFIG_HOME: configHome,
      AIDC_REFRESH_LEEWAY_MS: "1000"
    }
  };
}

function runLink(root, env, clientId) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [entry, "link", "app-1", "--client", clientId], {
      cwd: root,
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

test("link refuses an OAuth client owned by a different app without writing config", async (t) => {
  const state = await fixture(t);
  const result = await runLink(state.root, state.env, "client-other");
  assert.equal(result.code, 1, result.stdout);
  assert.match(result.stderr, /belongs to another application/);
  assert.deepEqual(state.requests.map((request) => request.url), ["/apps/app-1", "/clients/client-other"]);
  await assert.rejects(fs.access(path.join(state.root, ".aid.json")), { code: "ENOENT" });
});

test("link validates and writes a client belonging to the selected app", async (t) => {
  const state = await fixture(t);
  const result = await runLink(state.root, state.env, "client-1");
  assert.equal(result.code, 0, result.stderr);
  const config = JSON.parse(await fs.readFile(path.join(state.root, ".aid.json"), "utf8"));
  assert.equal(config.app_id, "app-1");
  assert.equal(config.client_id, "client-1");
  assert.deepEqual(state.requests.map((request) => request.url), ["/apps/app-1", "/clients/client-1"]);
});
