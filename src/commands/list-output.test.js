import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import http from "node:http";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const entry = fileURLToPath(new URL("../../bin/aid.js", import.meta.url));

function runCli(args, env) {
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

test("resource list JSON is filterable, clean, and excludes client secrets", async (t) => {
  const server = http.createServer((req, res) => {
    if (req.headers.authorization !== "Bearer list-test-token") {
      res.writeHead(401, { "content-type": "application/json" });
      res.end(JSON.stringify({ message: "Unauthorized" }));
      return;
    }

    res.writeHead(200, { "content-type": "application/json" });
    if (req.url === "/apps") {
      res.end(JSON.stringify([
        { id: "app-1", name: "Production Web", description: "Customer portal", client_secret: "never-print-app-secret" },
        { id: "app-2", name: "Mobile", description: "Native app" },
        { id: "app-3", name: "Preview Web", description: "Staging" }
      ]));
      return;
    }
    if (req.url === "/clients") {
      res.end(JSON.stringify([
        { client_id: "web-client", name: "Web Frontend", public: true, app_id: "app-1", redirect_uris: ["https://example.test/callback"], scopes: ["openid"], client_secret: "never-print-client-secret" },
        { client_id: "mobile-client", name: "Mobile App", public: true, app_id: "app-2", redirect_uris: ["myapp://callback"], scopes: ["openid"] }
      ]));
      return;
    }
    res.writeHead(404).end();
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));

  const configHome = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-list-output-"));
  t.after(async () => {
    server.closeAllConnections?.();
    await new Promise((resolve) => server.close(resolve));
    await fs.rm(configHome, { recursive: true, force: true });
  });

  const sessionDir = path.join(configHome, "aidc");
  await fs.mkdir(sessionDir, { recursive: true });
  await fs.writeFile(path.join(sessionDir, "tokens.json"), JSON.stringify({
    access_token: "list-test-token",
    expires_at: Date.now() + 60_000
  }), { mode: 0o600 });

  const env = {
    ...process.env,
    AIDC_ISSUER: `http://127.0.0.1:${server.address().port}`,
    AIDC_REFRESH_LEEWAY_MS: "1000",
    XDG_CONFIG_HOME: configHome
  };

  const apps = await runCli(["apps", "list", "--search", "web", "--limit", "1", "--json"], env);
  assert.equal(apps.code, 0, apps.stderr);
  assert.deepEqual(JSON.parse(apps.stdout), [{
    id: "app-1",
    name: "Production Web",
    description: "Customer portal"
  }]);
  assert.doesNotMatch(apps.stdout, /never-print/);

  const clients = await runCli(["clients", "list", "--json"], env);
  assert.equal(clients.code, 0, clients.stderr);
  const parsedClients = JSON.parse(clients.stdout);
  assert.equal(parsedClients.length, 2);
  assert.equal(parsedClients[0].client_id, "web-client");
  assert.equal(Object.hasOwn(parsedClients[0], "client_secret"), false);
  assert.doesNotMatch(clients.stdout, /never-print-client-secret/);
  assert.doesNotMatch(clients.stdout, /CLIENT ID/);
});
