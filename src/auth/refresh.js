import { ISSUER, CLIENT_ID, REQUEST_TIMEOUT_MS } from "./config.js";
import {
  loadTokens,
  saveTokensUnlocked,
  isExpiringSoon,
  clearTokensUnlocked,
  withTokenLock
} from "./tokens.js";
import { readApiError } from "../api/errors.js";

let refreshPromise = null;

export async function getValidAccessToken() {
  const tokens = await loadTokens();
  if (!tokens?.access_token) {
    throw new Error("Not signed in. Run `aid login` first.");
  }
  if (!isExpiringSoon(tokens)) return tokens.access_token;

  if (!refreshPromise) {
    refreshPromise = withTokenLock(async () => {
      // A different CLI process may have refreshed while this process waited.
      const latest = await loadTokens();
      if (!latest?.access_token) {
        throw new Error("Not signed in. Run `aid login` first.");
      }
      if (!isExpiringSoon(latest)) return latest;
      if (!latest.refresh_token) {
        await clearTokensUnlocked();
        throw new Error("Session expired and no refresh token is stored. Run `aid login` again.");
      }
      return refresh(latest);
    }).finally(() => {
      refreshPromise = null;
    });
  }

  const refreshed = await refreshPromise;
  return refreshed.access_token;
}

async function refresh(tokens) {
  let response;
  try {
    response = await fetchWithTimeout(`${ISSUER}/token`, {
      method: "POST",
      redirect: "error",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        grant_type: "refresh_token",
        refresh_token: tokens.refresh_token,
        client_id: CLIENT_ID
      })
    });
  } catch (err) {
    if (err.name === "AbortError" || err.name === "TimeoutError") {
      throw new Error(`Token refresh timed out after ${REQUEST_TIMEOUT_MS / 1000}s.`);
    }
    throw new Error("Token refresh request failed or was redirected unexpectedly.");
  }

  if (!response.ok) {
    if (response.status === 400 || response.status === 401) {
      await response.body?.cancel().catch(() => {});
      await clearTokensUnlocked();
      throw new Error("Refresh failed because the session is no longer valid. Run `aid login` again.");
    }
    const detail = await readApiError(response);
    throw new Error(`Token refresh failed (${response.status}): ${detail}`);
  }

  let body;
  try {
    body = await response.json();
  } catch (err) {
    if (err.name === "AbortError" || err.name === "TimeoutError") {
      throw new Error(`Token refresh response timed out after ${REQUEST_TIMEOUT_MS / 1000}s.`);
    }
    throw new Error("Token refresh returned invalid JSON. Run `aid login` again.");
  }

  if (typeof body?.access_token !== "string" || !body.access_token.trim()) {
    throw new Error("Token refresh returned no access token. Run `aid login` again.");
  }

  const expiresIn = Number(body.expires_in ?? 3600);
  const updated = {
    access_token: body.access_token,
    refresh_token: body.refresh_token || tokens.refresh_token,
    id_token: body.id_token || tokens.id_token,
    token_type: body.token_type || tokens.token_type || "Bearer",
    scope: body.scope || tokens.scope,
    expires_at: Date.now() + (Number.isFinite(expiresIn) && expiresIn > 0 ? expiresIn : 3600) * 1000
  };

  await saveTokensUnlocked(updated);
  return updated;
}

async function fetchWithTimeout(url, options) {
  return fetch(url, { ...options, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
}
