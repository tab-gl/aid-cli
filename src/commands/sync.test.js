import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import http from "node:http";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const entry = fileURLToPath(new URL("../../bin/aid.js", import.meta.url));

async function fixture(t, config, overrides = {}) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-sync-project-"));
  const configHome = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-sync-auth-"));
  const requests = [];
  let signalClientRequest;
  const clientRequest = new Promise((resolve) => { signalClientRequest = resolve; });
  const server = http.createServer(async (req, res) => {
    requests.push({ method: req.method, url: req.url });
    if (req.url === "/apps/app-1") {
      if (overrides.appWaitsForClient) await clientRequest;
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify(overrides.app ?? { id: "app-1", name: "Remote app name", description: "Remote description" }));
      return;
    }
    if (req.url === "/clients/client-1") {
      signalClientRequest();
      if (overrides.delayClientMs) await new Promise((resolve) => setTimeout(resolve, overrides.delayClientMs));
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify(overrides.client ?? {
        client_id: "client-1",
        redirect_uris: ["https://example.test/new-callback"],
        scopes: ["openid", "profile", "openid"]
      }));
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

  const issuer = `http://127.0.0.1:${server.address().port}`;
  const sessionDir = path.join(configHome, "aidc");
  await fs.mkdir(sessionDir, { recursive: true });
  await fs.writeFile(path.join(sessionDir, "tokens.json"), JSON.stringify({
    access_token: "sync-test-token",
    expires_at: Date.now() + 3_600_000
  }), { mode: 0o600 });
  if (config !== undefined) await fs.writeFile(path.join(root, ".aid.json"), config);
  return {
    root, configHome, issuer, requests, clientRequest,
    env: { ...process.env, AIDC_ISSUER: issuer, XDG_CONFIG_HOME: configHome, AIDC_REFRESH_LEEWAY_MS: "1000" }
  };
}

function runSync(root, env, args = []) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [entry, "sync", ...args], {
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

test("sync refreshes remote app and client metadata while preserving unrelated project settings", async (t) => {
  const state = await fixture(t);
  const configPath = path.join(state.root, ".aid.json");
  const original = {
    issuer: state.issuer,
    app_id: "app-1",
    client_id: "client-1",
    redirect_uri: "https://example.test/old-callback",
    scopes: ["openid"],
    framework: "Vite",
    custom_setting: { keep: true }
  };
  await fs.writeFile(configPath, JSON.stringify(original));

  const result = await runSync(state.root, state.env);
  assert.equal(result.code, 0, result.stderr);
  assert.match(result.stdout, /Remote app name/);
  assert.deepEqual(state.requests.map((request) => request.url).sort(), ["/apps/app-1", "/clients/client-1"]);

  const synced = JSON.parse(await fs.readFile(configPath, "utf8"));
  assert.equal(synced.app_name, "Remote app name");
  assert.equal(synced.app_description, "Remote description");
  assert.equal(synced.redirect_uri, "https://example.test/new-callback");
  assert.deepEqual(synced.scopes, ["openid", "profile"]);
  assert.deepEqual(synced.custom_setting, { keep: true });
  assert.equal(synced.framework, "Vite");
  assert.equal(typeof synced.synced_at, "string");
});

test("sync refuses a different issuer without touching local config", async (t) => {
  const state = await fixture(t);
  const configPath = path.join(state.root, ".aid.json");
  const original = JSON.stringify({ issuer: "https://other-issuer.example", app_id: "app-1" }, null, 2);
  await fs.writeFile(configPath, original);

  const result = await runSync(state.root, state.env);
  assert.equal(result.code, 1);
  assert.match(result.stderr, /different issuer/);
  assert.deepEqual(state.requests, []);
  assert.equal(await fs.readFile(configPath, "utf8"), original);
});

test("sync refuses malformed config rather than replacing it", async (t) => {
  const state = await fixture(t);
  const configPath = path.join(state.root, ".aid.json");
  const original = "{ definitely not JSON";
  await fs.writeFile(configPath, original);

  const result = await runSync(state.root, state.env);
  assert.equal(result.code, 1);
  assert.match(result.stderr, /invalid JSON/);
  assert.deepEqual(state.requests, []);
  assert.equal(await fs.readFile(configPath, "utf8"), original);
});

test("sync removes stale metadata when remote values are explicitly cleared", async (t) => {
  const state = await fixture(t, undefined, {
    app: { id: "app-1", name: null, description: null },
    client: { client_id: "client-1", redirect_uris: [], scopes: [] }
  });
  const configPath = path.join(state.root, ".aid.json");
  await fs.writeFile(configPath, JSON.stringify({
    issuer: state.issuer,
    app_id: "app-1",
    client_id: "client-1",
    app_name: "Old name",
    app_description: "Old description",
    redirect_uri: "https://example.test/old",
    scopes: ["openid"],
    custom_setting: "preserve"
  }));

  const result = await runSync(state.root, state.env);
  assert.equal(result.code, 0, result.stderr);
  const synced = JSON.parse(await fs.readFile(configPath, "utf8"));
  assert.equal(Object.hasOwn(synced, "app_name"), false);
  assert.equal(Object.hasOwn(synced, "app_description"), false);
  assert.equal(Object.hasOwn(synced, "redirect_uri"), false);
  assert.deepEqual(synced.scopes, []);
  assert.equal(synced.custom_setting, "preserve");
});

test("sync rejects a client linked to another app without changing local config", async (t) => {
  const state = await fixture(t, undefined, {
    client: {
      client_id: "client-1",
      app_id: "another-app",
      redirect_uris: ["https://example.test/callback"],
      scopes: ["openid"]
    }
  });
  const configPath = path.join(state.root, ".aid.json");
  const original = JSON.stringify({
    issuer: state.issuer,
    app_id: "app-1",
    client_id: "client-1",
    custom_setting: "keep"
  });
  await fs.writeFile(configPath, original);

  const result = await runSync(state.root, state.env);
  assert.equal(result.code, 1);
  assert.match(result.stderr, /belongs to another application/);
  assert.equal(await fs.readFile(configPath, "utf8"), original);
});

test("sync refuses to overwrite local edits made while remote metadata is loading", async (t) => {
  const state = await fixture(t, undefined, { delayClientMs: 150 });
  const configPath = path.join(state.root, ".aid.json");
  const original = JSON.stringify({
    issuer: state.issuer,
    app_id: "app-1",
    client_id: "client-1",
    custom_setting: "before"
  });
  await fs.writeFile(configPath, original);

  const pending = runSync(state.root, state.env);
  await state.clientRequest;
  const edited = JSON.stringify({
    issuer: state.issuer,
    app_id: "app-1",
    client_id: "client-1",
    custom_setting: "changed while sync was running"
  }, null, 2);
  await fs.writeFile(configPath, edited);

  const result = await pending;
  assert.equal(result.code, 1);
  assert.match(result.stderr, /changed while sync was running/);
  assert.equal(await fs.readFile(configPath, "utf8"), edited);
});

test("sync starts independent app and client metadata requests concurrently", async (t) => {
  const state = await fixture(t, undefined, { appWaitsForClient: true });
  const configPath = path.join(state.root, ".aid.json");
  await fs.writeFile(configPath, JSON.stringify({
    issuer: state.issuer,
    app_id: "app-1",
    client_id: "client-1"
  }));

  const result = await runSync(state.root, state.env);
  assert.equal(result.code, 0, result.stderr);
  assert.deepEqual(state.requests.map((request) => request.url).sort(), [
    "/apps/app-1",
    "/clients/client-1"
  ]);
});


test("sync dry-run reports exact metadata changes without writing the config", async (t) => {
  const state = await fixture(t);
  const configPath = path.join(state.root, ".aid.json");
  const original = JSON.stringify({
    issuer: state.issuer,
    app_id: "app-1",
    client_id: "client-1",
    app_name: "Old app name",
    app_description: "Remote description",
    redirect_uri: "https://example.test/old-callback",
    scopes: ["openid"],
    keep: "untouched"
  }, null, 2);
  await fs.writeFile(configPath, original);

  const result = await runSync(state.root, state.env, ["--dry-run"]);
  assert.equal(result.code, 0, result.stderr);
  assert.match(result.stdout, /Application name: "Old app name" -> "Remote app name"/);
  assert.match(result.stdout, /Redirect URI: "https:\\/\\/example.test\\/old-callback" -> "https:\\/\\/example.test\\/new-callback"/);
  assert.ok(result.stdout.includes('Scopes: ["openid"] -> ["openid","profile"]'));
  assert.match(result.stdout, /No files were changed/);
  assert.equal(await fs.readFile(configPath, "utf8"), original);
});

test("sync does not rewrite config when remote metadata is unchanged", async (t) => {
  const state = await fixture(t, undefined, {
    app: { id: "app-1", name: "Remote app name", description: "Remote description" },
    client: { client_id: "client-1", redirect_uris: ["https://example.test/new-callback"], scopes: ["openid", "profile"] }
  });
  const configPath = path.join(state.root, ".aid.json");
  const original = JSON.stringify({
    issuer: state.issuer,
    app_id: "app-1",
    client_id: "client-1",
    app_name: "Remote app name",
    app_description: "Remote description",
    redirect_uri: "https://example.test/new-callback",
    scopes: ["openid", "profile"],
    synced_at: "keep-this",
    custom_setting: true
  }, null, 2);
  await fs.writeFile(configPath, original);

  const result = await runSync(state.root, state.env);
  assert.equal(result.code, 0, result.stderr);
  assert.match(result.stdout, /No metadata changes detected/);
  assert.equal(await fs.readFile(configPath, "utf8"), original);
});
