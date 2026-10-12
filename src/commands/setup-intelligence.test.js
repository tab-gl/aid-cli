import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const entry = fileURLToPath(new URL("../../bin/aid.js", import.meta.url));

async function projectFixture(t, { config, packageJson, installedSdk } = {}) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "aid-cli-setup-intelligence-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  if (packageJson) await fs.writeFile(path.join(root, "package.json"), JSON.stringify(packageJson, null, 2));
  if (installedSdk) {
    const sdkDir = path.join(root, "node_modules", "ace-id-sdk");
    await fs.mkdir(sdkDir, { recursive: true });
    await fs.writeFile(path.join(sdkDir, "package.json"), JSON.stringify({ name: "ace-id-sdk", version: installedSdk }));
  }
  const configPath = path.join(root, ".aid.json");
  await fs.writeFile(configPath, JSON.stringify(config, null, 2));
  return { root, configPath, original: await fs.readFile(configPath, "utf8") };
}

function runSetup(root) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [entry, "setup"], {
      cwd: root,
      env: { ...process.env, AIDC_ISSUER: "https://identity.ace-base.cc" },
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

test("setup detects installed SDK version and avoids recreating a healthy setup", async (t) => {
  const state = await projectFixture(t, {
    packageJson: { name: "web-project", dependencies: { next: "^15.0.0", "ace-id-sdk": "^2.0.0" } },
    installedSdk: "2.1.3",
    config: {
      issuer: "https://identity.ace-base.cc",
      app_id: "app-1",
      client_id: "client-1",
      project_type: "next",
      framework: "Next.js",
      sdk: "ace-id-sdk"
    }
  });

  const result = await runSetup(state.root);
  assert.equal(result.code, 0, result.stderr);
  assert.match(result.stdout, /SDK RANGE.*ace-id-sdk: \^2\.0\.0/);
  assert.match(result.stdout, /SDK VERSION.*Installed: 2\.1\.3/);
  assert.match(result.stdout, /No local project\/configuration drift detected/);
  assert.equal(await fs.readFile(state.configPath, "utf8"), state.original);
});

test("setup reports drift and minimal repair without changing existing resources or config", async (t) => {
  const state = await projectFixture(t, {
    packageJson: { name: "web-project", dependencies: { next: "^15.0.0" } },
    config: {
      issuer: "https://identity.ace-base.cc",
      app_id: "app-1",
      client_id: "client-1",
      project_type: "react",
      framework: "React",
      sdk: "old-sdk"
    }
  });

  const result = await runSetup(state.root);
  assert.equal(result.code, 0, result.stderr);
  const output = result.stdout + result.stderr;
  assert.match(output, /Configuration drift detected/);
  assert.match(output, /SDK dependency ace-id-sdk is not declared/);
  assert.match(output, /npm install ace-id-sdk/);
  assert.match(output, /aid link app-1 --force/);
  assert.equal(await fs.readFile(state.configPath, "utf8"), state.original);
});


test("setup detects an installed SDK version outside the declared range", async (t) => {
  const state = await projectFixture(t, {
    packageJson: { name: "web-project", dependencies: { next: "^15.0.0", "ace-id-sdk": "^2.0.0" } },
    installedSdk: "3.0.0",
    config: {
      issuer: "https://identity.ace-base.cc",
      app_id: "app-1",
      client_id: "client-1",
      project_type: "next",
      framework: "Next.js",
      sdk: "ace-id-sdk"
    }
  });

  const result = await runSetup(state.root);
  const output = result.stdout + result.stderr;
  assert.equal(result.code, 0, result.stderr);
  assert.match(output, /Installed SDK version 3\.0\.0 does not satisfy declared range \^2\.0\.0/);
  assert.match(output, /npm install ace-id-sdk/);
  assert.equal(await fs.readFile(state.configPath, "utf8"), state.original);
});
