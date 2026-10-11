import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import http from "node:http";
import { spawn } from "node:child_process";

test("concurrent CLI processes refresh a rotating token only once", async (t) => {
  let refreshCount = 0;
  const server = http.createServer(async (req, res) => {
    if (req.method !== "POST" || req.url !== "/token") {
      res.writeHead(404).end();
      return;
    }
    refreshCount++;
    await new Promise((resolve) => setTimeout(resolve, 120));
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({
      access_token: "fresh-access-token",
      refresh_token: "rotated-refresh-token",
      expires_in: 3600,
      token_type: "Bearer"
    }));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));

  const configHome = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-refresh-"));
  t.after(async () => {
    server.closeAllConnections?.();
    await new Promise((resolve) => server.close(resolve));
    await fs.rm(configHome, { recursive: true, force: true });
  });

  const sessionDir = path.join(configHome, "aidc");
  await fs.mkdir(sessionDir, { recursive: true });
  await fs.writeFile(path.join(sessionDir, "tokens.json"), JSON.stringify({
    access_token: "expired-access-token",
    refresh_token: "original-refresh-token",
    expires_at: Date.now() - 1000
  }), { mode: 0o600 });

  const moduleUrl = new URL("./refresh.js", import.meta.url).href;
  const script = `
    import { getValidAccessToken } from ${JSON.stringify(moduleUrl)};
    console.log(await getValidAccessToken());
  `;
  const env = {
    ...process.env,
    AIDC_ISSUER: `http://127.0.0.1:${server.address().port}`,
    AIDC_REQUEST_TIMEOUT_MS: "3000",
    AIDC_REFRESH_LEEWAY_MS: "1000",
    XDG_CONFIG_HOME: configHome
  };

  function runRefresh() {
    return new Promise((resolve, reject) => {
      const child = spawn(process.execPath, ["--input-type=module", "-e", script], {
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
      child.once("close", (code) => {
        if (code !== 0) reject(new Error(`refresh process failed (${code}): ${stderr}`));
        else resolve(stdout.trim());
      });
    });
  }

  const results = await Promise.all([runRefresh(), runRefresh()]);
  assert.deepEqual(results, ["fresh-access-token", "fresh-access-token"]);
  assert.equal(refreshCount, 1, "only one process should use the rotating refresh token");
  const stored = JSON.parse(await fs.readFile(path.join(sessionDir, "tokens.json"), "utf8"));
  assert.equal(stored.refresh_token, "rotated-refresh-token");
});
