import { ISSUER } from "../auth/config.js";
import { getValidAccessToken } from "../auth/refresh.js";

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
  const accessToken = await getValidAccessToken();
  const url = resolveApiUrl(path);
  const headers = {
    ...(options.headers || {}),
    authorization: `Bearer ${accessToken}`
  };
  const response = await fetch(url, { ...options, headers });
  if (response.status === 401) {
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
  return response.json();
}
