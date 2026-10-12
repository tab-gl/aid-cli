import test from "node:test";
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import http from "node:http";

function resolveWithIssuer(issuer, requestPath) {
  const script = `
    import { resolveApiUrl } from ${JSON.stringify(new URL("./url.js", import.meta.url).href)};
    try {
      console.log(resolveApiUrl(${JSON.stringify(requestPath)}).href);
    } catch (error) {
      console.error(error.message);
      process.exitCode = 1;
    }
  `;
  return spawnSync(process.execPath, ["--input-type=module", "-e", script], {
    encoding: "utf8",
    env: { ...process.env, AIDC_ISSUER: issuer }
  });
}

test("keeps API calls inside an issuer path prefix", () => {
  const result = resolveWithIssuer("https://identity.example/base", "/apps");
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout.trim(), "https://identity.example/base/apps");
});

test("rejects absolute and protocol-relative request paths", () => {
  assert.notEqual(resolveWithIssuer("https://identity.example", "https://evil.example/steal").status, 0);
  assert.notEqual(resolveWithIssuer("https://identity.example", "//evil.example/steal").status, 0);
});

test("rejects plain and encoded path traversal outside the issuer prefix", () => {
  assert.notEqual(resolveWithIssuer("https://identity.example/base", "../steal").status, 0);
  assert.notEqual(resolveWithIssuer("https://identity.example/base", "%2e%2e/steal").status, 0);
});

test("rejects empty and non-string paths", () => {
  assert.notEqual(resolveWithIssuer("https://identity.example", "").status, 0);
  assert.notEqual(resolveWithIssuer("https://identity.example", "   ").status, 0);
});


test("request timeout remains active while consuming a stalled response body", async (t) => {
  const server = http.createServer((_req, res) => {
    res.writeHead(200, { "content-type": "application/json" });
    res.flushHeaders();
    // Deliberately leave the response body open to simulate a stalled API.
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));

  const configHome = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-timeout-"));
  t.after(async () => {
    server.closeAllConnections?.();
    await new Promise((resolve) => server.close(resolve));
    await fs.rm(configHome, { recursive: true, force: true });
  });

  await fs.mkdir(path.join(configHome, "aidc"), { recursive: true });
  await fs.writeFile(path.join(configHome, "aidc", "tokens.json"), JSON.stringify({
    access_token: "test-token",
    expires_at: Date.now() + 60_000
  }));

  const port = server.address().port;
  const script = `
    import { apiJson } from ${JSON.stringify(new URL("./client.js", import.meta.url).href)};
    try {
      await apiJson("/slow");
      console.error("request unexpectedly succeeded");
      process.exitCode = 2;
    } catch (error) {
      console.error(error.message);
      process.exitCode = /Response body timed out/.test(error.message) ? 0 : 1;
    }
  `;
  const child = spawn(process.execPath, ["--input-type=module", "-e", script], {
    env: {
      ...process.env,
      AIDC_ISSUER: `http://127.0.0.1:${port}`,
      AIDC_REQUEST_TIMEOUT_MS: "1000",
      AIDC_REFRESH_LEEWAY_MS: "1000",
      XDG_CONFIG_HOME: configHome
    },
    stdio: ["ignore", "pipe", "pipe"]
  });

  let stderr = "";
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk) => { stderr += chunk; });
  const exitCode = await new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("close", resolve);
  });

  assert.equal(exitCode, 0, stderr);
  assert.match(stderr, /Response body timed out/);
});
