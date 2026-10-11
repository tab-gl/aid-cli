import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";

const configUrl = new URL("./config.js", import.meta.url);

function loadConfig(env) {
  return spawnSync(process.execPath, ["--input-type=module", "-e", `
    import * as c from ${JSON.stringify(configUrl.href)};
    console.log(JSON.stringify({ issuer: c.ISSUER, port: c.CALLBACK_PORT, redirect: c.REDIRECT_URI, timeout: c.REQUEST_TIMEOUT_MS }));
  `], { encoding: "utf8", env: { ...process.env, ...env } });
}

test("uses safe default issuer and callback configuration", () => {
  const result = loadConfig({ AIDC_ISSUER: "", AIDC_PORT: "", AIDC_REQUEST_TIMEOUT_MS: "" });
  assert.equal(result.status, 0, result.stderr);
  const config = JSON.parse(result.stdout);
  assert.equal(config.issuer, "https://identity.ace-base.cc");
  assert.equal(config.port, 7357);
  assert.equal(config.redirect, "http://127.0.0.1:7357/callback");
  assert.equal(config.timeout, 15000);
});

test("rejects malformed issuer URLs and insecure non-local issuers", () => {
  for (const issuer of ["not-a-url", "http://example.com", "https://user:pass@example.com", "https://example.com?x=1"]) {
    const result = loadConfig({ AIDC_ISSUER: issuer });
    assert.notEqual(result.status, 0, `issuer should be rejected: ${issuer}`);
  }
});

test("rejects invalid port and timeout configuration instead of silently falling back", () => {
  assert.notEqual(loadConfig({ AIDC_PORT: "70000" }).status, 0);
  assert.notEqual(loadConfig({ AIDC_REQUEST_TIMEOUT_MS: "-2" }).status, 0);
});
