import { spawn, spawnSync } from "node:child_process";

import {
  ISSUER,
  CLIENT_ID,
  REDIRECT_URI,
  SCOPES
} from "./config.js";

import {
  generateVerifier,
  challengeFor,
  generateState
} from "./pkce.js";

import { waitForCallback } from "./callback.js";
import { saveTokens } from "./tokens.js";
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

  const s = spinner(
    "Waiting for you to authorize in the browser…",
    { interval: 100 }
  ).start();

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
    response = await fetch(`${ISSUER}/token`, {
      method: "POST",
      headers: {
        "content-type": "application/x-www-form-urlencoded"
      },
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
    throw err;
  }

  if (!response.ok) {
    const text = await response.text();

    s.fail(`Token exchange failed (${response.status}).`);

    throw new Error(text);
  }

  const body = await response.json();

  await saveTokens({
    access_token: body.access_token,
    refresh_token: body.refresh_token,
    id_token: body.id_token,
    token_type: body.token_type || "Bearer",
    scope: body.scope,
    expires_at:
      Date.now() + (body.expires_in ?? 3600) * 1000
  });

  s.succeed("Signed in. Tokens stored.");
}

function openBrowser(url) {
  const platform = process.platform;

  let cmd;
  let args;

  if (platform === "darwin") {
    cmd = "open";
    args = [url];
  } else if (platform === "win32") {
    cmd = "cmd";
    args = ["/c", "start", "", url];
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
