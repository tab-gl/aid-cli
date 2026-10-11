import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";

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
