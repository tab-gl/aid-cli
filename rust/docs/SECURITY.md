# Security notes

## Threat model

The CLI handles three secrets: the OAuth access token, the refresh token, and (for
confidential clients) the client secret printed once after creation. The assets it
protects are the local session file, the local project configuration, and the
credentials sent to the Ace ID API.

In scope: a hostile network (redirects, slow responses, oversized bodies), another
local process (token file, lock file), a hostile API response (error text injected
into the terminal), and untrusted project content (`.aid.json`, `package.json`).

Out of scope: a compromised local user account or root, a compromised issuer, and
malicious SDK release artifacts (mitigated by pinned hashes, but trust ultimately
rests on the release publisher).

## Controls

| Control | Implementation |
| --- | --- |
| CSPRNG for PKCE and state | `getrandom` (OS entropy) |
| PKCE S256 | SHA-256 over the verifier, base64url without padding |
| State comparison | constant-time equality (`util::constant_time_eq`) |
| Single-use callback | the listener returns as soon as a valid code arrives and is then dropped |
| Loopback only | listener bound to `127.0.0.1`, never `0.0.0.0` |
| No secrets in logs | tokens are never printed; error details pass through `sanitize_detail` (Bearer values, `access_token`/`refresh_token`/`id_token`/`client_secret`/`authorization`/`api_key`/`password` assignments, and URLs are redacted) |
| Terminal escape protection | `sanitize_detail` drops C0 controls (other than `\r\n\t`, which become spaces), DEL and C1 controls, so a hostile issuer cannot emit cursor moves, title changes or hyperlinks |
| Restrictive permissions | token and `.aid.json` files `0600`, config directory `0700` (POSIX) |
| Atomic writes | temp file + `fsync` + `rename`, `create_new` so an existing temp file is never reused |
| Symlink resistance | token files are read with `symlink_metadata`; symlinks and non-regular files are refused |
| Refresh race protection | in-process mutex + cross-process lock file + re-read after acquiring the lock |
| URL validation | issuer must be HTTPS (except loopback), with no credentials, query or fragment; API paths are resolved against the issuer origin and containment-checked after normalisation |
| Redirect rejection | every credentialed request (API, token endpoint) uses `redirects(0)`; a 3xx is an error, never followed |
| Bounded redirects for downloads | the unauthenticated SDK metadata/artifact requests may follow at most 5 redirects (GitHub renames repositories and serves assets via a CDN); integrity is enforced by the pinned SHA-256, not by the transport |
| Proxy support | `HTTPS_PROXY`/`HTTP_PROXY` are honoured, `NO_PROXY` respected, and loopback targets are never proxied |
| Issuer pinning | `tokens.json` records its issuer; a session from another issuer is refused instead of being sent to the wrong service |
| Timeouts | one deadline per request covering connect, headers and body reads |
| Bounded bodies | API responses capped at 8 MiB, error details at 4 KiB / 240 chars |
| Artifact verification | Android SDK: release `digest` must equal `sha256:<pin>` **and** the downloaded bytes must hash to the same pin before writing |
| No untrusted execution | the CLI never runs remote content; package installs use the project's own package manager with a fixed package name and no shell |
| Path safety | `paths::ensure_within` refuses writes outside the project directory |

## Reporting a vulnerability

Please report privately to the maintainers rather than opening a public issue.
Include the version (`aid --version`), the platform and a reproduction.

## Residual risks

- The session file is protected by file permissions, not encryption. Anyone who can
  read your home directory can use your session; use full-disk encryption.
- `AIDC_ISSUER` may point at any HTTPS host, and the token store is shared across
  issuers. Pointing the CLI at a different issuer means the same `tokens.json`
  holds a session for that issuer; `sync` refuses to operate on a project linked to
  another issuer, but `login` will overwrite the stored session.
- The lock file is advisory. A crash at exactly the wrong moment can leave a stale
  lock; it is recovered automatically once the owning pid is gone or after 60 s,
  and the error message tells you which file to delete if recovery ever fails.
- TLS trust uses the bundled `webpki-roots` set. A TLS-inspecting proxy (corporate
  MITM, some CI sandboxes) fails with `tls connection init failed`; there is no
  `AIDC_CA_BUNDLE` yet. Workaround for the Android SDK: download
  `library-release.aar` manually and place it in `libs/` — `setup`'s inspection
  verifies the pinned hash.
- Terminal escape injection from API error text is now stripped (see above), but
  *other* untrusted text (app names, client names, descriptions) is printed
  verbatim, as in v1. Treat a hostile issuer's data as hostile.
