import test from "node:test";
import assert from "node:assert/strict";
import { registerStyle, spinner } from "./spinner.js";

function outputStream(isTTY = false) {
  let output = "";
  return {
    isTTY,
    columns: 32,
    write(value) { output += value; },
    read() { return output; }
  };
}

test("non-TTY spinner writes only a final status line", () => {
  const stream = outputStream(false);
  spinner("Loading", { stream }).start().succeed("Loaded");
  assert.equal(stream.read(), "  ✓ Loaded\n");
});

test("spinner start and finish are idempotent", () => {
  const stream = outputStream(false);
  const task = spinner("Loading", { stream });
  task.start().start().succeed("Done").fail("Late failure");
  assert.equal(stream.read(), "  ✓ Done\n");
});

test("spinner rejects empty or malformed custom frames", () => {
  assert.throws(() => spinner("Loading", { frames: [] }), /non-empty array/);
  assert.throws(() => spinner("Loading", { frames: ["ok", ""] }), /non-empty strings/);
  assert.throws(() => registerStyle("", ["x"]), /style names/);
});

test("custom spinner styles are copied and can be selected", () => {
  const frames = ["A", "B"];
  registerStyle("test-style", frames);
  frames.push("C");
  const stream = outputStream(false);
  spinner("Working", { style: "test-style", stream }).succeed();
  assert.equal(stream.read(), "  ✓ Working\n");
});
