export const ISSUER =
  process.env.AIDC_ISSUER ||
  "https://identity.ace-base.cc";

export const CLIENT_ID =
  process.env.AIDC_CLIENT_ID ||
  "aidc-cli";

export const CALLBACK_PORT =
  Number(process.env.AIDC_PORT) || 7357;

export const REDIRECT_URI =
  `http://127.0.0.1:${CALLBACK_PORT}/callback`;

export const SCOPES =
  "openid profile email offline_access";

export const CALLBACK_TIMEOUT_MS =
  5 * 60 * 1000;

export const REFRESH_LEEWAY_MS =
  60 * 1000;
