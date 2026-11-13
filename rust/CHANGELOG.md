# Changelog — Ace ID CLI v2 (Rust)

This changelog covers the Rust implementation in `rust/` only. The v1 Node.js
changelog is at `../CHANGELOG.md` and continues independently.

## 2.0.0

First release of the Rust implementation. Feature parity with v1 `0.1.4`.

### Added

- Single static-ish binary `aid` (no Node.js runtime); `ace-id` and `aidc` aliases
  accepted as a leading argument.
- All v1 commands and flags: `setup` (incl. `--interactive`, `--wizard`,
  `--recover`, `--force`), `login`, `logout`, `whoami`, `test-auth`, `apps`
  (`list`/`create`/`get`/`update`/`delete`), `clients`
  (`list`/`create`/`get`/`delete`/`rotate-secret`), `link`, `sync`, `version`,
  `help`.
- Compatibility aliases: `init` → `setup`, `app` → `apps`, bare
  `list`/`create`/`get`/`update`/`delete` → `apps`.
- Command chaining on `;`, `;;`, `\;`.
- OAuth 2.0 Authorization Code + PKCE (S256) with state validation, single-use
  loopback callback, browser hand-off and 5-minute callback timeout.
- Token store shared with v1 (`aidc/tokens.json`), atomic writes, `0600`/`0700`
  modes, symlink and type validation, cross-process refresh lock, in-process
  refresh de-duplication.
- API client with issuer-pinned URL resolution, redirect rejection, request
  deadlines covering body reads, bounded bodies and redacted error details.
- Project detection (Next.js, Vite, React, vanilla, Android Kotlin/Java),
  `setup` drift inspection with repair hints, recovery journal and rollback,
  `link` and `sync` (including `--dry-run` and concurrent-edit protection).
- SDK handling: package-manager installs for JS projects, pinned-and-verified
  Android artifact download.
- Responsive terminal output: banner, tables, key/value blocks, command lists,
  spinner, colour with `NO_COLOR` support.
- Monochrome terminal palette: one neutral near-white ramp carries structure and
  body text, and colour is reserved for the two status exceptions — success
  (green) and errors (red). Warnings stay monochrome.
- Test suite: 117 unit tests + 65 integration tests, including an end-to-end
  login against a loopback mock, concurrent refresh, recovery and cleanup paths.
- Rust CI workflow (`.github/workflows/rust-v2.yml`) with fmt, clippy, tests,
  release build, cross-target builds and checksummed GitHub Releases for `v2*` tags.

### Hardening (pre-release, from the round-2 review)

- `--json` output and `.aid.json` keep v1's key order (insertion order), so v2 no
  longer reorders the user's project file.
- Request bodies are serialised in v1's key order.
- `help` spacing, timeout message formatting (`0.3s`, not `1s`) and the read order
  of `sync` / `clients list` now match v1 exactly.
- Stored sessions record their issuer and are refused for a different
  `AIDC_ISSUER`; v1-shaped sessions keep working.
- Error details from an untrusted issuer can no longer contain terminal control
  characters.
- SDK downloads follow up to 5 redirects (needed after the SDK repository was
  renamed and because assets are served from a CDN); credentialed requests still
  refuse redirects.
- `HTTPS_PROXY` / `HTTP_PROXY` / `NO_PROXY` are honoured; loopback is never proxied.
- `AIDC_SDK_RELEASE_BASE` allows installing the Android SDK from a mirror.
- Verified against the real v1 CLI (102 differential cases), a simultaneous v1/v2
  refresh race, the live SDK release and a real `npm install`; see `REVIEW.md`.
- Redirects are followed manually: hop budget, `https → http` downgrade refusal and
  credential stripping on every hop.
- Fixed the Windows token write: `std::fs::rename` does not overwrite there, so the
  second write (any refresh) would have failed; `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`
  is used instead.
- Empty-string arguments (`aid apps get ""`, `aid link ""`, …) are usage errors, as in v1.
- Storage failures now report what could not be written instead of a bare OS error.
- Documented limitation: proxy authentication (407) is not supported by the HTTP client.

### Changed relative to v1

- `--help` and `--version` no longer fail when `AIDC_ISSUER` is invalid
  (configuration is resolved lazily).
- A failed `login` prints its error once instead of twice.
- Missing/non-string API fields render as `—` in tables and are omitted from
  key/value blocks instead of `undefined` / `[object Object]`.
- Browser detection scans `PATH` instead of spawning `sh -c "command -v …"`.
- `sync` and `clients list` perform their two reads sequentially rather than
  concurrently (same output, one extra round trip).
- Version string is `ace-id 2.0.0`.

### Not included

- No npm publication; v1 remains the npm package.
- No changes to v1 source, workflows or release assets.
- Cross-platform binaries are built by CI; only Linux x86_64 was built, tested and
  measured in the environment used to prepare this release (see `AUDIT.md`).
