import { ISSUER, CLIENT_ID } from "./config.js";
import {
  loadTokens,
  saveTokens,
  isExpiringSoon
} from "./tokens.js";

export async function getValidAccessToken() {
  const tokens = await loadTokens();

  if (!tokens?.access_token) {
    throw new Error("Not signed in. Run `aidc login` first.");
  }

  if (!isExpiringSoon(tokens)) {
    return tokens.access_token;
  }

  if (!tokens.refresh_token) {
    throw new Error(
      "Session expired and no refresh token is stored. " +
      "Run `aidc login` again."
    );
  }

  const refreshed = await refresh(tokens);
  return refreshed.access_token;
}

async function refresh(tokens) {
  const response = await fetch(`${ISSUER}/token`, {
    method: "POST",
    headers: {
      "content-type": "application/x-www-form-urlencoded"
    },
    body: new URLSearchParams({
      grant_type: "refresh_token",
      refresh_token: tokens.refresh_token,
      client_id: CLIENT_ID
    })
  });

  if (!response.ok) {
    const text = await response.text();

    if (response.status === 400) {
      throw new Error(
        "Refresh failed — your session has expired. " +
        "Run `aidc login` again."
      );
    }

    throw new Error(
      `Token refresh failed (${response.status}): ${text}`
    );
  }

  const body = await response.json();

  const updated = {
    access_token: body.access_token,
    refresh_token: body.refresh_token || tokens.refresh_token,
    id_token: body.id_token || tokens.id_token,
    token_type: body.token_type || tokens.token_type || "Bearer",
    scope: body.scope || tokens.scope,
    expires_at:
      Date.now() + (body.expires_in ?? 3600) * 1000
  };

  await saveTokens(updated);
  return updated;
}
