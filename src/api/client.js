import { ISSUER, REQUEST_TIMEOUT_MS } from "../auth/config.js";
import { getValidAccessToken } from "../auth/refresh.js";
import { clearTokens } from "../auth/tokens.js";

const ISSUER_URL = new URL(ISSUER);
const ISSUER_ORIGIN = ISSUER_URL.origin;
const ISSUER_BASE_PATH = ISSUER_URL.pathname.replace(/\/$/, "");

function resolveApiUrl(requestPath) {
  if (typeof requestPath !== "string" || !requestPath.trim()) {
    throw new TypeError("API path must be a non-empty string.");
  }

  const path = requestPath.replace(/^\/+/, "");
  if (/^[a-z][a-z0-9+.-]*:/i.test(path) || path.startsWith("//")) {
    throw new Error("Refusing API request outside the configured Ace ID issuer.");
  }

  const base = `${ISSUER_BASE_PATH}/`;
  const url = new URL(path, new URL(base, ISSUER_ORIGIN));
  if (url.origin !== ISSUER_ORIGIN || !url.pathname.startsWith(base)) {
    throw new Error("Refusing API request outside the configured Ace ID issuer.");
  }
  return url;
}

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
    if (err.name === "AbortError") {
      throw new Error(`Request timed out after ${REQUEST_TIMEOUT_MS / 1000}s.`);
    }
    if (err.name === "TypeError" && /redirect/i.test(err.message)) {
      throw new Error("API request was redirected unexpectedly; refusing to forward credentials.");
    }
    throw new Error(`Network request failed: ${err.message}`);
  }

  if (response.status === 401) {
    await clearTokens().catch(() => {});
    throw new Error("Server rejected the access token. Run `aid login` again.");
  }
  return response;
}

export async function apiJson(path, options = {}) {
  const response = await apiFetch(path, options);
  if (!response.ok) {
    const text = await response.text();
    const detail = text.length > 500 ? `${text.slice(0, 500)}…` : text;
    throw new Error(`Request failed (${response.status}): ${detail}`);
  }

  try {
    return await response.json();
  } catch {
    throw new Error(`Request succeeded but returned invalid JSON (HTTP ${response.status}).`);
  }
}

async function fetchWithTimeout(url, options) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
  try {
    return await fetch(url, { ...options, signal: controller.signal });
  } finally {
    clearTimeout(timer);
  }
}
