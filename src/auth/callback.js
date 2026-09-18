import http from "node:http";

import {
  CALLBACK_PORT,
  CALLBACK_TIMEOUT_MS
} from "./config.js";

export function waitForCallback(expectedState) {
  return new Promise((resolve, reject) => {
    let settled = false;
    let timer = null;

    const server = http.createServer(handleRequest);
    server.on("error", handleServerError);

    timer = setTimeout(handleTimeout, CALLBACK_TIMEOUT_MS);
    timer.unref?.();

    server.listen(CALLBACK_PORT, "127.0.0.1");

    function handleRequest(req, res) {
      const url = new URL(
        req.url,
        `http://127.0.0.1:${CALLBACK_PORT}`
      );

      if (url.pathname !== "/callback") {
        res.statusCode = 404;
        res.end();
        return;
      }

      const state = url.searchParams.get("state");
      const code = url.searchParams.get("code");
      const error = url.searchParams.get("error");

      if (error) {
        respond(res, failureHtml("Sign-in failed"));
        settle(reject, new Error(`Authorization failed: ${error}`));
        return;
      }

      if (state !== expectedState) {
        respond(res, failureHtml("Invalid state"));
        settle(reject, new Error("State mismatch — possible CSRF."));
        return;
      }

      if (!code) {
        respond(res, failureHtml("Missing code"));
        settle(reject, new Error("No authorization code in callback."));
        return;
      }

      respond(res, successHtml());
      settle(resolve, code);
    }

    function handleServerError(err) {
      if (err.code === "EADDRINUSE") {
        settle(
          reject,
          new Error(
            `Port ${CALLBACK_PORT} is already in use. ` +
            `Close whatever is using it and run again.`
          )
        );
      } else {
        settle(reject, err);
      }
    }

    function handleTimeout() {
      settle(
        reject,
        new Error(
          `Timed out after ${CALLBACK_TIMEOUT_MS / 1000}s ` +
          `waiting for the browser.`
        )
      );
    }

    function settle(fn, value) {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      server.close(() => fn(value));
    }
  });
}

function respond(res, html) {
  try {
    res.statusCode = 200;
    res.setHeader("content-type", "text/html; charset=utf-8");
    res.end(html);
  } catch {
    // Response already closed.
  }
}

function successHtml() {
  return `<!doctype html><html><head><meta charset="utf-8">
<title>Signed in</title>
<style>body{font-family:-apple-system,system-ui,sans-serif;max-width:420px;margin:20vh auto;padding:24px;color:#111;background:#f5f5f2;text-align:center}h1{font-size:24px;margin:0 0 12px}p{color:#666;line-height:1.6}</style>
</head><body><h1>Signed in</h1>
<p>You can close this tab and return to your terminal.</p>
</body></html>`;
}

function failureHtml(message) {
  return `<!doctype html><html><head><meta charset="utf-8">
<title>Sign-in failed</title>
<style>body{font-family:-apple-system,system-ui,sans-serif;max-width:420px;margin:20vh auto;padding:24px;color:#111;background:#f5f5f2;text-align:center}h1{font-size:24px;margin:0 0 12px}p{color:#666;line-height:1.6}</style>
</head><body><h1>${message}</h1>
<p>Return to your terminal for details.</p>
</body></html>`;
}
