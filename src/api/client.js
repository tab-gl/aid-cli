import { ISSUER } from "../auth/config.js";
import { getValidAccessToken } from "../auth/refresh.js";

export async function apiFetch(path, options = {}) {
  const accessToken = await getValidAccessToken();

  const url = path.startsWith("http") ? path : `${ISSUER}${path}`;

  const headers = {
    ...(options.headers || {}),
    authorization: `Bearer ${accessToken}`
  };

  const response = await fetch(url, { ...options, headers });

  if (response.status === 401) {
    throw new Error(
      "Server rejected the access token. Run `aidc login` again."
    );
  }

  return response;
}

export async function apiJson(path, options = {}) {
  const response = await apiFetch(path, options);

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Request failed (${response.status}): ${text}`);
  }

  return response.json();
}
