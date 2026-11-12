# Authentication

v2 implements the same OpenID Connect **Authorization Code flow with PKCE
(S256)** as v1, against the same issuer and endpoints.

## Flow

1. `aid login` generates:
   - a **code verifier**: 32 CSPRNG bytes, base64url, 43 characters;
   - a **code challenge**: `BASE64URL(SHA-256(verifier))`;
   - a **state**: 16 CSPRNG bytes, base64url, 22 characters.
2. It binds a loopback listener on `127.0.0.1:<AIDC_PORT>` (default `7357`)
   **before** printing the authorize URL, so the browser cannot arrive early.
3. It prints and opens:

   ```text
   <AIDC_ISSUER>/auth?response_type=code&client_id=aidc-cli
     &redirect_uri=http://127.0.0.1:7357/callback
     &scope=openid profile email offline_access
     &state=<state>&code_challenge=<challenge>&code_challenge_method=S256
   ```

   The browser opener follows v1: `$BROWSER`, then `open` (macOS),
   `explorer.exe` (Windows), `termux-open-url` (Termux), `xdg-open` (Linux). If
   none is available the URL is only printed.
4. The browser is redirected to `http://127.0.0.1:7357/callback?code=…&state=…`.
5. The CLI validates the request:
   - `GET` on `/callback` only, otherwise `404`;
   - `state` must match exactly (compared in constant time), otherwise `400
     Invalid state` and the login keeps waiting — a forged request cannot burn
     the pending login;
   - `error` in the query → `Authorization was rejected by the identity provider.`;
   - missing `code` → `No authorization code in callback.`;
   - a valid callback is consumed once and the listener closes.
6. The code is exchanged at `<AIDC_ISSUER>/token` with
   `grant_type=authorization_code`, `code`, `redirect_uri`, `client_id` and
   `code_verifier`. Redirects are refused; the request has a hard deadline.
7. Tokens are written atomically to the token file and the CLI prints
   `Signed in. Tokens stored.`

Callback timeout defaults to 5 minutes (`AIDC_CALLBACK_TIMEOUT_MS`).

## Stored session

| Platform | Path |
| --- | --- |
| Linux/BSD/macOS | `$XDG_CONFIG_HOME/aidc/tokens.json`, else `~/.config/aidc/tokens.json` |
| Windows | `%APPDATA%\aidc\tokens.json` |

```json
{
  "access_token": "…",
  "refresh_token": "…",
  "id_token": "…",
  "token_type": "Bearer",
  "scope": "openid profile email",
  "expires_at": 1794474975000
}
```

- File mode `0600`, directory mode `0700` (POSIX).
- Writes go to a unique temp file created with `create_new`, are `fsync`ed, then
  renamed into place, so a crash never truncates the session.
- Unknown keys written by another client are preserved.
- v2 records `"issuer"` in the file and refuses to use a session created for a
  different `AIDC_ISSUER` (v1 sessions have no such key and are accepted).
- Symlinks, non-regular files, invalid JSON and wrong field types are rejected
  with `Stored Ace ID session is invalid or unsafe. Run \`aid login\` again.`

## Switching issuers

`aid login` always replaces the stored session. If `AIDC_ISSUER` points somewhere
else than the stored session's issuer, commands fail with

```text
✗ The stored session belongs to https://old.example. Run `aid login` to sign in to https://new.example.
```

rather than sending the old token to the new service. Nothing is deleted, so
switching `AIDC_ISSUER` back restores the previous session.

## Refresh

An access token is refreshed when `now + AIDC_REFRESH_LEEWAY_MS >= expires_at`
(default leeway 60 s), or when it has no `expires_at`.

Coordination happens in two layers:

1. **In-process:** a mutex makes concurrent threads share one refresh.
2. **Cross-process:** a lock file (`tokens.json.lock`, mode 0600, holding
   `{pid, created_at}`) serialises refreshes between CLI processes. A lock is
   treated as stale when its owning pid is gone, or after 60 s; the lock identity
   (inode/mtime) is re-checked before unlinking. After acquiring the lock the
   token file is re-read, so a process that waited on another's refresh simply
   reuses the new token instead of refreshing twice.

Refresh outcomes:

| Result | Behaviour |
| --- | --- |
| New access token | merged over the stored tokens (missing `refresh_token`, `scope`, `token_type` fall back to the previous values) |
| HTTP 400/401 | tokens deleted, `Refresh failed because the session is no longer valid. Run \`aid login\` again.` |
| Other HTTP error | error reported, tokens kept |
| Timeout / transport error | error reported, tokens kept |
| No stored `refresh_token` | tokens deleted, `Session expired and no refresh token is stored. Run \`aid login\` again.` |

An API `401` also deletes the session, but only if the token file still holds the
same access token that was rejected (so a concurrent successful refresh is never
undone).

## Unauthenticated commands

`help`, `version`, `login`, `logout` and `setup --recover` (journal-only paths) do
not need a session. `setup`, `whoami`, `test-auth`, `apps`, `clients`, `link` and
`sync` do.
