import test from "node:test";
import assert from "node:assert/strict";

import { readApiError } from "./errors.js";

test("formats JSON API errors without exposing credentials or URLs", async () => {
  const response = new Response(JSON.stringify({
    message: "Bearer abc123 access_token=secret-value at https://identity.example/private?token=abc"
  }), {
    status: 403,
    headers: { "content-type": "application/json" }
  });

  const message = await readApiError(response);
  assert.match(message, /^Forbidden:/);
  assert.doesNotMatch(message, /abc123|secret-value|identity\.example|token=abc/);
  assert.match(message, /REDACTED/);
  assert.match(message, /\[URL\]/);
});

test("does not print HTML error pages", async () => {
  const response = new Response("<html><body>private internal stack trace</body></html>", {
    status: 502,
    headers: { "content-type": "text/html" }
  });
  assert.equal(await readApiError(response), "HTTP error");
});

test("bounds long API error messages", async () => {
  const response = new Response(JSON.stringify({ message: "x".repeat(10_000) }), {
    status: 500,
    headers: { "content-type": "application/json" }
  });
  const message = await readApiError(response);
  assert.ok(message.length <= 260);
});
