import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import http from "node:http";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const entry = fileURLToPath(new URL("../../bin/aid.js", import.meta.url));

async function fixture(t, configure) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-setup-recovery-"));
  const configHome = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-setup-auth-"));
  const requests = [];
  const server = http.createServer((req, res) => {
    requests.push({ method: req.method, url: req.url });
    if (req.method === "DELETE" && /^\/apps\//.test(req.url)) {
      res.writeHead(204).end();
      return;
    }
    res.writeHead(404).end();
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
    access_token: "test-access-token",
    expires_at: Date.now() + 3_600_000
  }), { mode: 0o600 });

  const env = {
    ...process.env,
    AIDC_ISSUER: `http://127.0.0.1:${server.address().port}`,
    XDG_CONFIG_HOME: configHome,
    AIDC_REFRESH_LEEWAY_MS: "1000"
  };
  await configure({ root, env });
  return { root, env, requests };
}

function runSetupRecover(root, env) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [entry, "setup", "--recover", "--force"], {
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

test("setup recovery removes recorded remote resources after interruption", async (t) => {
  const state = await fixture(t, async ({ root }) => {
    await fs.writeFile(path.join(root, ".aid-setup-recovery.json"), JSON.stringify({
      issuer: "placeholder",
      app_id: "app-left-behind",
      client_id: null,
      stage: "cleanup-required"
    }));
  });
  const recordPath = path.join(state.root, ".aid-setup-recovery.json");
  const record = JSON.parse(await fs.readFile(recordPath, "utf8"));
  record.issuer = new URL(state.env.AIDC_ISSUER).origin;
  await fs.writeFile(recordPath, JSON.stringify(record));

  const result = await runSetupRecover(state.root, state.env);
  assert.equal(result.code, 0, result.stderr);
  assert.deepEqual(state.requests, [{ method: "DELETE", url: "/apps/app-left-behind" }]);
  await assert.rejects(fs.access(recordPath), { code: "ENOENT" });
});

test("recovery never deletes resources already referenced by completed config", async (t) => {
  const state = await fixture(t, async ({ root }) => {
    await fs.writeFile(path.join(root, ".aid-setup-recovery.json"), JSON.stringify({
      issuer: "pending",
      app_id: "app-complete",
      client_id: "client-complete",
      stage: "resources-created"
    }));
  });
  const issuer = state.env.AIDC_ISSUER;
  await fs.writeFile(path.join(state.root, ".aid-setup-recovery.json"), JSON.stringify({
    issuer,
    app_id: "app-complete",
    client_id: "client-complete",
    stage: "resources-created"
  }));
  await fs.writeFile(path.join(state.root, ".aid.json"), JSON.stringify({
    issuer,
    app_id: "app-complete",
    client_id: "client-complete"
  }));

  const result = await runSetupRecover(state.root, state.env);
  assert.equal(result.code, 0, result.stderr);
  assert.deepEqual(state.requests, []);
  await assert.rejects(fs.access(path.join(state.root, ".aid-setup-recovery.json")), { code: "ENOENT" });
});
