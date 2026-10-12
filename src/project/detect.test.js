import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const moduleUrl = new URL("./detect.js", import.meta.url).href;

function detectIn(root) {
  return spawnSync(process.execPath, ["--input-type=module", "-e", `
    import { detectProject } from ${JSON.stringify(moduleUrl)};
    try {
      console.log(JSON.stringify(await detectProject()));
    } catch (error) {
      console.error(error.message);
      process.exitCode = 1;
    }
  `], { cwd: root, encoding: "utf8" });
}

test("project detection reports malformed package.json instead of silently choosing vanilla JS", async (t) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-detect-invalid-json-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  await fs.writeFile(path.join(root, "package.json"), '{"dependencies": ');
  const result = detectIn(root);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /package\.json contains invalid JSON/);
});

test("project detection rejects malformed dependency manifest fields", async (t) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-detect-invalid-deps-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  await fs.writeFile(path.join(root, "package.json"), JSON.stringify({ dependencies: "react" }));
  const result = detectIn(root);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /dependencies.*must be an object/);
});
