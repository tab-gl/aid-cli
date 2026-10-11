import test from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { waitForCallback } from "./callback.js";
import { CALLBACK_PORT } from "./config.js";

function request(path, method = "GET") {
  return new Promise((resolve, reject) => {
    const req = http.request({ hostname: "127.0.0.1", port: CALLBACK_PORT, path, method }, (res) => {
      res.resume();
      res.on("end", () => resolve(res.statusCode));
    });
    req.on("error", reject);
    req.end();
  });
}

async function waitListening() {
  for (let i = 0; i < 80; i++) {
    try {
      const status = await request("/");
      if (status) return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  }
  throw new Error("Callback server did not start.");
}

test("rejects mismatched and wrong-method requests without cancelling login", async () => {
  const callback = waitForCallback("expected-state");
  await waitListening();
  assert.equal(await request("/callback?state=wrong&code=attacker"), 400);
  assert.equal(await request("/callback?state=expected-state", "POST"), 404);
  assert.equal(await request("/callback?state=expected-state&code=valid-code"), 200);
  assert.equal(await callback, "valid-code");
});
