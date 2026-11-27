import test from "node:test";
import assert from "node:assert/strict";

import { openBrowser } from "./login.js";

test("a failed browser spawn is handled without crashing login", async () => {
  const originalBrowser = process.env.BROWSER;
  process.env.BROWSER = `/definitely-missing-browser-${process.pid}`;

  try {
    const child = openBrowser("https://example.invalid/sign-in");
    const error = await new Promise((resolve) => child.once("error", resolve));
    assert.equal(error.code, "ENOENT");
  } finally {
    if (originalBrowser === undefined) delete process.env.BROWSER;
    else process.env.BROWSER = originalBrowser;
  }
});
