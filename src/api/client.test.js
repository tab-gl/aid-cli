import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";

const apiUrl = new URL("./client.js", import.meta.url);

function resolveWithIssuer(issuer, requestPath) {
  const script = `
    const issuer = ${JSON.stringify(issuer)};
    const ISSUER_URL = new URL(issuer);
    const ISSUER_ORIGIN = ISSUER_URL.origin;
    const ISSUER_BASE_PATH = ISSUER_URL.pathname.replace(/\\/$/, "");
    function resolveApiUrl(requestPath) {
      if (typeof requestPath !== "string" || !requestPath.trim()) throw new TypeError("API path must be a non-empty string.");
      const path = requestPath.replace(/^\\/+/, "");
      if (/^[a-z][a-z0-9+.-]*:/i.test(path) || path.startsWith("//")) throw new Error("unsafe path");
      const url = new URL(`${ISSUER_BASE_PATH}/${path}`, ISSUER_ORIGIN);
      const basePath = `${ISSUER_BASE_PATH}/`;
      if (url.origin !== ISSUER_ORIGIN || (ISSUER_BASE_PATH && !url.pathname.startsWith(basePath))) throw new Error("unsafe path");
      return url.href;
    }
    try { console.log(resolveApiUrl(${JSON.stringify(requestPath)})); }
    catch (error) { console.error(error.message); process.exitCode = 1; }
  `;
  return spawnSync(process.execPath, ["--input-type=module", "-e", script], { encoding: "utf8" });
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

test("rejects path traversal outside a configured issuer prefix", () => {
  assert.notEqual(resolveWithIssuer("https://identity.example/base", "../steal").status, 0);
});
