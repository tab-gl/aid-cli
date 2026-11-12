# v1 reference implementation audit

Scope: the Node.js CLI on `master` (package `ace-id` 0.1.4), audited before any
Rust code was written. Findings drive `PARITY.md` and the deliberate divergences
listed at the end.

## Repository state at audit time

- Default branch is **`master`**, not `main`. The task brief said "treat `main` as
  the reference implementation"; `master` is that branch here. `main` does not
  exist in this repository.
- Latest commit on `master` at audit time: `74ee869 Add GitHub Actions workflow
  for npm package publishing`.
- 66 tracked files, ~5.6k lines including tests. Node 18+, ESM, no runtime
  dependencies, `node --test` for tests, `scripts/check.js` for a syntax pass.
- CI (`.github/workflows/ci.yml`) runs on `master` pushes and all PRs: syntax
  check, tests, `npm pack --dry-run`, alias smoke tests, Node 18/20/22.
- `npm-publish.yml` publishes the npm package on release. v2 must not touch it.

## Structure of v1

| Area | Files |
| --- | --- |
| Entry point, aliases, chaining, help | `bin/aid.js`, `bin/ace-id.js`, `bin/aidc.js` |
| Config from env | `src/auth/config.js` |
| PKCE, state | `src/auth/pkce.js` |
| Loopback callback server | `src/auth/callback.js` |
| Login flow, browser hand-off | `src/auth/login.js` |
| Token store, lock, validation | `src/auth/tokens.js` |
| Refresh + cross-process coordination | `src/auth/refresh.js` |
| API client, URL resolution, error rendering | `src/api/{client,url,errors}.js` |
| Commands | `src/commands/**` |
| Project detection | `src/project/detect.js` |
| Terminal output | `src/ui/{colors,format,banner,screen,spinner,progress}.js` |

## Behaviour documented during the audit

- **Token location:** `%APPDATA%\aidc\tokens.json` on Windows, else
  `$XDG_CONFIG_HOME/aidc/tokens.json` or `~/.config/aidc/tokens.json`.
- **Token document:** `{access_token, refresh_token?, id_token?, token_type?,
  scope?, expires_at?}`; unknown keys are preserved across writes.
- **`.aid.json`:** `{issuer, app_id, client_id?, redirect_uri?, scopes?,
  project_type, framework, sdk, sdk_path?, sdk_version?, app_name?,
  app_description?, synced_at?}`; written with mode 0600, two-space JSON, trailing
  newline. `sync` only rewrites the metadata keys it owns.
- **Recovery journal:** `.aid-setup-recovery.json` with
  `{issuer, app_id, client_id, created_at, stage}` where stage ∈
  `app-created | resources-created | cleanup-required`.
- **Lock file:** `<tokens.json>.lock`, `wx` create, `{pid, created_at}`, stale
  after 60 s or when the owning pid is gone, 30 s acquire timeout.
- **Exit codes:** 0 success, 1 for every handled failure.
- **stdout vs stderr:** `success`/`info`/`fields`/`table` → stdout;
  `failure`/`warn` → stderr.

## Bugs and rough edges found in v1

Each item is either fixed in v2 or reproduced intentionally.

1. **Duplicate error output after a failed login.** `login.js` calls
   `s.fail(err.message)` and then rethrows, so the top-level handler prints the
   same message again. v2 clears the spinner and prints the error once.
2. **`sh -c "command -v <bin>"` for browser detection** (`login.js`
   `hasCommand`). The binary names are constants so it is not exploitable, but it
   is a shell invocation for a PATH lookup. v2 scans `PATH` directly and never
   spawns a shell.
3. **Configuration is evaluated at import time.** `src/auth/config.js` throws at
   module load, so `aid --version` and `aid --help` fail when `AIDC_ISSUER` is
   invalid. v2 resolves configuration lazily, per command that needs it.
4. **`Promise.all` for two independent reads** in `sync.js` and
   `clients/list.js`. v2 reads sequentially; identical results, one extra round
   trip. Kept simple rather than adding an async runtime for two requests.
5. **`String(value)` on non-string API fields.** `whoami`/`test-auth` render
   objects as `[object Object]` and missing ids as `undefined` in tables. v2
   omits non-string fields from key/value blocks and prints `—` for missing table
   cells (v1's own table convention for null/undefined).
6. **`refresh.js` clears tokens on 400/401 but not on other failures.** v2 keeps
   that behaviour (and only clears when the session is genuinely invalid).
7. **`apps get`/`clients get` accept `--json` only as the second token.** v2
   preserves this exact parsing.
8. **Version string comes from `package.json` at runtime.** v2 compiles the
   version in (`env!("CARGO_PKG_VERSION")`), so the binary is self-contained; the
   printed format (`ace-id 2.0.0`) is unchanged.

## Security review of v1 (carried into v2)

Verified as correct and ported as-is:

- PKCE verifier/challenge lengths and S256 derivation; state is 128 bits of CSPRNG
  output.
- Callback listener bound to `127.0.0.1` only; non-GET and non-`/callback`
  requests are answered 404; wrong-state requests are rejected without settling,
  so a forged request cannot burn the pending login.
- Token writes are temp-file + `rename` with `wx`, mode 0600, directory 0700,
  symlinks and non-regular files rejected on read.
- Stale-lock recovery re-checks the lock identity (inode/mtime) before unlinking.
- API paths are resolved against the issuer origin and containment-checked after
  normalisation, so traversal cannot escape the issuer.
- Redirects are refused at the HTTP layer for the API, token endpoints and SDK
  downloads.
- Error details are bounded (4 KiB read, 240 chars rendered), HTML bodies
  suppressed, tokens/credentials/URLs redacted.
- Android SDK artifact is verified twice: the release `digest` from the GitHub API
  and a recomputed SHA-256 of the downloaded bytes, against a pinned constant.

Gaps found in v1 that v2 also inherits (documented, not silently "fixed"):

- **No CSRF/`Origin` check on the callback** beyond state validation. State is
  unguessable and single-use, so this is acceptable; v2 additionally compares
  state in constant time.
- **Lock file is advisory.** Two processes racing on a stale lock can both enter
  briefly; v2 mitigates with identity re-checks and by re-reading tokens after
  acquiring the lock, which is what makes the race harmless.
- **`AIDC_ISSUER` may point at any HTTPS host.** This is by design (self-hosted
  issuers) and the token store is shared, so users switching issuers share one
  session file. v2 keeps this and `sync` refuses to act on a project linked to a
  different issuer.

## Compatibility constraints derived from the audit

1. Command names, aliases, flags, short flags and usage strings must not change.
2. `tokens.json` and `.aid.json` must keep their paths, modes and JSON shape.
3. Environment variables and their defaults must not change.
4. The OAuth flow must keep `client_id=aidc-cli`, `redirect_uri=http://127.0.0.1:<port>/callback`
   and the `/auth`, `/token`, `/me`, `/apps`, `/clients` endpoints.
5. v1 must remain buildable and releasable from `master`; v2 lives only in
   `rust/` on `v2-rust`.
