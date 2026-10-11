import { ISSUER } from "../auth/config.js";

const ISSUER_URL = new URL(ISSUER);
const ISSUER_ORIGIN = ISSUER_URL.origin;
const ISSUER_BASE_PATH = ISSUER_URL.pathname.replace(/\/+$/, "");

export function resolveApiUrl(requestPath) {
  if (typeof requestPath !== "string" || !requestPath.trim()) {
    throw new TypeError("API path must be a non-empty string.");
  }

  const rawPath = requestPath.trim();
  if (/^[a-z][a-z0-9+.-]*:/i.test(rawPath) || rawPath.startsWith("//")) {
    throw new Error("Refusing API request outside the configured Ace ID issuer.");
  }

  const relativePath = rawPath.replace(/^\/+/, "");
  const base = new URL(`${ISSUER_BASE_PATH}/`, ISSUER_ORIGIN);
  const url = new URL(relativePath, base);

  // URL normalizes dot segments, so the containment check happens after
  // resolution and catches encoded/plain path traversal as well.
  if (url.origin !== ISSUER_ORIGIN || !url.pathname.startsWith(base.pathname)) {
    throw new Error("Refusing API request outside the configured Ace ID issuer.");
  }

  return url;
}
