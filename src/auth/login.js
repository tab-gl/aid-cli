import { spawn, spawnSync } from "node:child_process";

import {
  ISSUER,
  CLIENT_ID,
  REDIRECT_URI,
  SCOPES,
  REQUEST_TIMEOUT_MS
} from "./config.js";

import {
  generateVerifier,
  challengeFor,
  generateState
} from "./pkce.js";

import { waitForCallback } from "./callback.js";
import { saveTokens } from "./tokens.js";
import { readApiError } from "../api/errors.js";
import { spinner } from "../ui/spinner.js";
import { info } from "../ui/format.js";

export async function login() {
  const verifier = generateVerifier();
  const challenge = challengeFor(verifier);
  const state = generateState();

  const authorizeUrl = new URL(`${ISSUER}/auth`);
  authorizeUrl.searchParams.set("response_type", "code");
  authorizeUrl.searchParams.set("client_id", CLIENT_ID);
  authorizeUrl.searchParams.set("redirect_uri", REDIRECT_URI);
  authorizeUrl.searchParams.set("scope", SCOPES);
  authorizeUrl.searchParams.set("state", state);
  authorizeUrl.searchParams.set("code_challenge", challenge);
  authorizeUrl.searchParams.set("code_challenge_method", "S256");

  const codePromise = waitForCallback(state);

  info("Opening browser to sign in…");
  info("If it doesn't open automatically, visit:");
  console.log("  " + authorizeUrl.toString() + "\n");

  openBrowser(authorizeUrl.toString());

  const s = spinner("Waiting for you to authorize in the browser…", { interval: 100 }).start();
  let code;
  try {
    code = await codePromise;
  } catch (err) {
    s.fail(err.message);
    throw err;
  }

  s.update("Exchanging authorization code for tokens…");

  let response;
  try {
    response = await fetchWithTimeout(`${ISSUER}/token`, {
      method: "POST",
      redirect: "error",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        grant_type: "authorization_code",
        code,
        redirect_uri: REDIRECT_URI,
        client_id: CLIENT_ID,
        code_verifier: verifier
      })
    });
  } catch (err) {
    s.fail("Network error during token exchange.");
    if (err.name === "AbortError" || err.name === "TimeoutError") {
      throw new Error(`Token exchange timed out after ${REQUEST_TIMEOUT_MS / 1000}s.`);
    }
    if (err.name === "TypeError") {
      throw new Error("Token endpoint request failed or was redirected unexpectedly.");
    }
    throw new Error("Token exchange request failed. Check your connection and try again.");
  }

  if (!response.ok) {
    const detail = await readApiError(response);
    s.fail(`Token exchange failed (${response.status}).`);
    throw new Error(`Token exchange failed (${response.status}): ${detail}`);
  }

  let body;
  try {
    body = await response.json();
  } catch (err) {
    if (err.name === "AbortError" || err.name === "TimeoutError") {
      s.fail("Token endpoint response timed out.");
      throw new Error(`Token endpoint response timed out after ${REQUEST_TIMEOUT_MS / 1000}s.`);
    }
    s.fail("Token endpoint returned invalid JSON.");
    throw new Error("Token endpoint returned invalid JSON. Run `aid login` again.");
  }

  if (typeof body?.access_token !== "string" || !body.access_token.trim()) {
    s.fail("Token endpoint returned no access token.");
    throw new Error("Token endpoint returned no access token.");
  }

  const expiresIn = Number(body.expires_in ?? 3600);
  await saveTokens({
    access_token: body.access_token,
    refresh_token: body.refresh_token,
    id_token: body.id_token,
    token_type: body.token_type || "Bearer",
    scope: body.scope,
    expires_at: Date.now() + (Number.isFinite(expiresIn) && expiresIn > 0 ? expiresIn : 3600) * 1000
  });

  s.succeed("Signed in. Tokens stored.");
}

function openBrowser(url) {
  const platform = process.platform;
  let cmd;
  let args;

  if (process.env.BROWSER) {
    const browser = process.env.BROWSER.trim();
    if (browser) {
      cmd = browser;
      args = [url];
    }
  } else if (platform === "darwin") {
    cmd = "open";
    args = [url];
  } else if (platform === "win32") {
    cmd = "explorer.exe";
    args = [url];
  } else if (hasCommand("termux-open-url")) {
    cmd = "termux-open-url";
    args = [url];
  } else if (hasCommand("xdg-open")) {
    cmd = "xdg-open";
    args = [url];
  } else {
    return;
  }

  try {
    spawn(cmd, args, { stdio: "ignore", detached: true }).unref();
  } catch {
    // URL is printed.
  }
}

function hasCommand(bin) {
  const result = process.platform === "win32"
    ? spawnSync("where", [bin], { stdio: "ignore" })
    : spawnSync("sh", ["-c", `command -v ${bin}`], { stdio: "ignore" });
  return result.status === 0;
}

async function fetchWithTimeout(url, options) {
  // AbortSignal.timeout remains active while fetch response bodies are read.
  return fetch(url, { ...options, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
}
