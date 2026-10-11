import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";

function resolveWithIssuer(issuer, requestPath) {
  const script = `
    const issuerUrl = new URL(${JSON.stringify(issuer)});
    const requestPath = ${JSON.stringify(requestPath)};
    if (typeof requestPath !== "string" || !requestPath.trim()) throw new Error("empty");
    if (/^[a-z][a-z0-9+.-]*:/i.test(requestPath) || requestPath.startsWith("//")) throw new Error("unsafe path");
    const path = requestPath.replace(/^\\/+/, "");
    const base = new URL(issuerUrl.pathname.replace(/\\/$/, "") + "/", issuerUrl.origin);
    const url = new URL(path, base);
    if (url.origin !== issuerUrl.origin || !url.pathname.startsWith(base.pathname)) throw new Error("unsafe path");
    console.log(url.href);
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
