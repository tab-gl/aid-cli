import test from "node:test";
import assert from "node:assert/strict";
import { createStepProgress } from "./progress.js";

function capture() {
  let output = "";
  return {
    stream: { isTTY: true, write(value) { output += value; } },
    read: () => output
  };
}

test("step progress shows the active step and advances as steps complete", () => {
  const output = capture();
  const progress = createStepProgress(["Detect project", "Check authentication", "Write config"], {
    stream: output.stream
  });

  progress.start(0);
  assert.match(output.read(), /7%\s+1\/3\s+Detect project/);
  progress.complete(0, "Project detected");
  assert.match(output.read(), /33%\s+1\/3\s+Project detected/);
  progress.start(1);
  progress.complete(1, "Authentication ready");
  assert.match(output.read(), /67%\s+2\/3\s+Authentication ready/);
});

test("step progress reports failures without claiming completion", () => {
  const output = capture();
  const progress = createStepProgress(["Detect project", "Create app"], { stream: output.stream });

  progress.start(0);
  progress.complete(0);
  progress.start(1);
  progress.fail("Application creation failed");

  assert.match(output.read(), /! \[.*\]\s+75%\s+2\/2\s+Application creation failed/);
  assert.doesNotMatch(output.read(), /100%\s+2\/2/);
});

test("step progress is silent when disabled", () => {
  const output = capture();
  const progress = createStepProgress(["One"], { stream: output.stream, enabled: false });
  progress.start(0);
  progress.complete(0);
  assert.equal(output.read(), "");
});

test("step progress rejects invalid indices and empty step lists", () => {
  const progress = createStepProgress(["One"], { stream: capture().stream });
  assert.throws(() => progress.start(1), /out of range/);
  assert.throws(() => progress.complete(-1), /out of range/);
  assert.throws(() => createStepProgress([]), /at least one step/);
});
