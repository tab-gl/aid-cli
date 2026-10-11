import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";

const entry = new URL("../bin/aid.js", import.meta.url);

function run(...args) {
  return spawnSync(process.execPath, [entry.pathname, ...args], { encoding: "utf8" });
}

test("help prints real newlines and command aliases", () => {
  const result = run("--help");
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /USAGE\n/);
  assert.match(result.stdout, /apps list/);
  assert.equal(result.stdout.includes("\\n"), false, "help must not print literal backslash-n sequences");
});

test("version reports package version", () => {
  const result = run("--version");
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /ace-id 0\.1\.4/);
});

test("unknown commands fail with a useful error", () => {
  const result = run("definitely-not-a-command");
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Unknown command/);
});
