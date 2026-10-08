const DEFAULT_ISSUER = "https://identity.ace-base.cc";

export const ISSUER = normalizeIssuer(
  process.env.AIDC_ISSUER || DEFAULT_ISSUER
);

export const CLIENT_ID =
  process.env.AIDC_CLIENT_ID || "aidc-cli";

export const CALLBACK_PORT =
  parsePort(process.env.AIDC_PORT) || 7357;

export const REDIRECT_URI =
  `http://127.0.0.1:${CALLBACK_PORT}/callback`;

export const SCOPES =
  process.env.AIDC_SCOPES ||
  "openid profile email offline_access";

export const CALLBACK_TIMEOUT_MS =
  parsePositiveInt(process.env.AIDC_CALLBACK_TIMEOUT_MS, 5 * 60 * 1000);

export const REFRESH_LEEWAY_MS =
  parsePositiveInt(process.env.AIDC_REFRESH_LEEWAY_MS, 60 * 1000);

export const REQUEST_TIMEOUT_MS =
  parsePositiveInt(process.env.AIDC_REQUEST_TIMEOUT_MS, 15 * 1000);

function normalizeIssuer(value) {
  const url = new URL(value);
  if (url.protocol !== "https:" && url.hostname !== "localhost" && url.hostname !== "127.0.0.1") {
    throw new Error("AIDC_ISSUER must use HTTPS unless it targets localhost.");
  }
  return url.origin;
}

function parsePort(value) {
  const port = Number(value);
  return Number.isInteger(port) && port >= 1 && port <= 65535 ? port : 0;
}

function parsePositiveInt(value, fallback) {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}
