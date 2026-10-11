import test from "node:test";
import assert from "node:assert/strict";
import { createStepProgress } from "./progress.js";

test("step progress renders readable completed and failed states", () => {
  let output = "";
  const stream = { isTTY: true, write(value) { output += value; } };
  const progress = createStepProgress(["Detect project", "Check authentication"], { stream, enabled: true });

  progress.start(0);
  progress.complete(0, "Project detected");
  progress.start(1);
  progress.fail("Authentication failed");

  assert.match(output, /\[░{16}\] 1\/2  Detect project/);
  assert.match(output, /\[████████░{8}\] 1\/2  Project detected/);
  assert.match(output, /2\/2  Check authentication/);
  assert.match(output, /Authentication failed/);
  assert.equal((output.match(/\n/g) || []).length, 4);
});

test("step progress is silent when disabled", () => {
  let output = "";
  const stream = { isTTY: false, write(value) { output += value; } };
  const progress = createStepProgress(["Detect project"], { stream, enabled: false });
  progress.start(0);
  progress.complete(0);
  progress.fail("failed");
  assert.equal(output, "");
});
