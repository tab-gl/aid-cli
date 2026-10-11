import http from "node:http";

import { CALLBACK_PORT, CALLBACK_TIMEOUT_MS } from "./config.js";

export function waitForCallback(expectedState) {
  return new Promise((resolve, reject) => {
    let settled = false;
    let listening = false;
    let timer;

    const server = http.createServer(handleRequest);
    server.on("error", handleServerError);

    timer = setTimeout(handleTimeout, CALLBACK_TIMEOUT_MS);
    server.listen(CALLBACK_PORT, "127.0.0.1");

    function handleRequest(req, res) {
      let url;
      try {
        url = new URL(req.url || "/", `http://127.0.0.1:${CALLBACK_PORT}`);
      } catch {
        respond(res, 400, failureHtml("Invalid callback request"));
        return;
      }

      if (req.method !== "GET" || url.pathname !== "/callback") {
        respond(res, 404, failureHtml("Not found"));
        return;
      }

      // Ignore forged/unrelated requests without consuming the one-time callback.
      if (url.searchParams.get("state") !== expectedState) {
        respond(res, 400, failureHtml("Invalid state"));
        return;
      }

      if (url.searchParams.has("error")) {
        respond(res, 400, failureHtml("Sign-in failed"));
        settle(reject, new Error("Authorization was rejected by the identity provider."));
        return;
      }

      const code = url.searchParams.get("code");
      if (!code) {
        respond(res, 400, failureHtml("Missing authorization code"));
        settle(reject, new Error("No authorization code in callback."));
        return;
      }

      respond(res, 200, successHtml());
      settle(resolve, code);
    }

    function handleServerError(err) {
      if (err.code === "EADDRINUSE") {
        settle(reject, new Error(
          `Port ${CALLBACK_PORT} is already in use. Close the conflicting process or set AIDC_PORT.`
        ));
      } else {
        settle(reject, err);
      }
    }

    function handleTimeout() {
      settle(reject, new Error(
        `Timed out after ${CALLBACK_TIMEOUT_MS / 1000}s waiting for the browser.`
      ));
    }

    function settle(fn, value) {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (!listening) {
        fn(value);
        return;
      }
      server.close(() => fn(value));
    }

    server.once("listening", () => {
      listening = true;
    });
  });
}

function respond(res, statusCode, html) {
  res.statusCode = statusCode;
  res.setHeader("content-type", "text/html; charset=utf-8");
  res.setHeader("cache-control", "no-store");
  res.setHeader("x-content-type-options", "nosniff");
  res.end(html);
}

function successHtml() {
  return `<!doctype html><html><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Signed in</title>
<style>body{font-family:system-ui,sans-serif;max-width:420px;margin:20vh auto;padding:24px;color:#111;background:#f5f5f2;text-align:center}h1{font-size:24px;margin:0 0 12px}p{color:#666;line-height:1.6}</style>
</head><body><h1>Signed in</h1><p>You can close this tab and return to your terminal.</p></body></html>`;
}

function failureHtml(message) {
  return `<!doctype html><html><head><meta charset="utf-8"><title>Sign-in failed</title></head><body><h1>${message}</h1></body></html>`;
}
