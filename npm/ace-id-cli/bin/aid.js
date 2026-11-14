#!/usr/bin/env node
"use strict";
const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const arch = { x64: "x64", arm64: "arm64", arm: "arm32" }[process.arch];
const exe = process.platform === "win32" ? "aid.exe" : "aid";
const bin = arch && path.join(__dirname, "..", process.platform, arch, exe);

if (!bin || !fs.existsSync(bin)) {
  console.error(`aid: unsupported platform ${process.platform}/${process.arch}`);
  process.exit(1);
}
if (process.platform !== "win32") {
  try { fs.chmodSync(bin, 0o755); } catch {}
}
const r = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
if (r.error) {
  console.error(`aid: failed to run ${bin}: ${r.error.message}`);
  process.exit(1);
}
process.exit(r.status === null ? 1 : r.status);
