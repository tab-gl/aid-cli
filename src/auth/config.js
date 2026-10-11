const DEFAULT_ISSUER = "https://identity.ace-base.cc";

export const ISSUER = normalizeIssuer(process.env.AIDC_ISSUER || DEFAULT_ISSUER);
export const CLIENT_ID = process.env.AIDC_CLIENT_ID?.trim() || "aidc-cli";

export const CALLBACK_PORT = parsePort(process.env.AIDC_PORT) ?? 7357;
export const REDIRECT_URI = `http://127.0.0.1:${CALLBACK_PORT}/callback`;
export const SCOPES = normalizeScopes(process.env.AIDC_SCOPES);

export const CALLBACK_TIMEOUT_MS = parsePositiveInt(process.env.AIDC_CALLBACK_TIMEOUT_MS, 5 * 60 * 1000);
export const REFRESH_LEEWAY_MS = parsePositiveInt(process.env.AIDC_REFRESH_LEEWAY_MS, 60 * 1000);
export const REQUEST_TIMEOUT_MS = parsePositiveInt(process.env.AIDC_REQUEST_TIMEOUT_MS, 15 * 1000);

function normalizeIssuer(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error("AIDC_ISSUER must be a valid absolute URL.");
  }

  const localHost = ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
  if (url.protocol !== "https:" && !(url.protocol === "http:" && localHost)) {
    throw new Error("AIDC_ISSUER must use HTTPS unless it targets localhost.");
  }
  if (url.username || url.password || url.search || url.hash) {
    throw new Error("AIDC_ISSUER must not include credentials, query parameters, or a fragment.");
  }
  return url.href.replace(/\/+$/, "");
}

function parsePort(value) {
  if (value === undefined || value === "") return null;
  const port = Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error("AIDC_PORT must be an integer between 1 and 65535.");
  }
  return port;
}

function parsePositiveInt(value, fallback) {
  if (value === undefined || value === "") return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error("Timeout and refresh-leeway environment values must be positive integers.");
  }
  return parsed;
}

function normalizeScopes(value) {
  const scopes = (value || "openid profile email offline_access").trim();
  if (!scopes) throw new Error("AIDC_SCOPES must contain a space-separated list of scopes.");
  return scopes;
}
