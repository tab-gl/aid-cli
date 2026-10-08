import { ISSUER, REQUEST_TIMEOUT_MS } from "../auth/config.js";
import { getValidAccessToken } from "../auth/refresh.js";
import { clearTokens } from "../auth/tokens.js";

const ISSUER_URL = new URL(ISSUER);
const ISSUER_ORIGIN = ISSUER_URL.origin;

function resolveApiUrl(path) {
  const url = new URL(path, ISSUER_URL);
  if (url.origin !== ISSUER_ORIGIN) {
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

  try {
    const response = await fetchWithTimeout(url, { ...options, headers });
    if (response.status === 401) {
      await clearTokens().catch(() => {});
      throw new Error("Server rejected the access token. Run `aid login` again.");
    }
    return response;
  } catch (err) {
    if (err.name === "AbortError") {
      throw new Error(`Request timed out after ${REQUEST_TIMEOUT_MS / 1000}s.`);
    }
    throw new Error(`Network request failed: ${err.message}`);
  }
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
