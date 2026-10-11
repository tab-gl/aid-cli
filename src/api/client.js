import { ISSUER, REQUEST_TIMEOUT_MS } from "../auth/config.js";
import { getValidAccessToken } from "../auth/refresh.js";
import { clearTokensIfAccessToken } from "../auth/tokens.js";
import { resolveApiUrl } from "./url.js";
import { readApiError } from "./errors.js";

export async function apiFetch(path, options = {}) {
  const url = resolveApiUrl(path);
  const accessToken = await getValidAccessToken();
  const headers = {
    ...(options.headers || {}),
    authorization: `Bearer ${accessToken}`
  };

  let response;
  try {
    response = await fetchWithTimeout(url, { ...options, headers, redirect: "error" });
  } catch (err) {
    if (err.name === "AbortError" || err.name === "TimeoutError") {
      throw new Error(`Request timed out after ${REQUEST_TIMEOUT_MS / 1000}s.`);
    }
    if (err.name === "TypeError" && /redirect/i.test(err.message)) {
      throw new Error("API request was redirected unexpectedly; refusing to forward credentials.");
    }
    throw new Error("Network request failed. Check your connection and try again.");
  }

  if (response.status === 401) {
    await clearTokensIfAccessToken(accessToken).catch(() => {});
    throw new Error("Server rejected the access token. Run `aid login` again.");
  }
  return response;
}

export async function apiJson(path, options = {}) {
  const response = await apiFetch(path, options);
  if (!response.ok) {
    const detail = await readApiError(response);
    throw new Error(`Request failed (${response.status}): ${detail}`);
  }

  try {
    return await response.json();
  } catch (err) {
    if (err.name === "AbortError" || err.name === "TimeoutError") {
      throw new Error(`Response body timed out after ${REQUEST_TIMEOUT_MS / 1000}s.`);
    }
    throw new Error(`Request succeeded but returned invalid JSON (HTTP ${response.status}).`);
  }
}

async function fetchWithTimeout(url, options) {
  // Keep the deadline active while the response body is consumed, not just
  // until headers arrive. Otherwise a stalled stream can hang the CLI forever.
  return fetch(url, { ...options, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
}
