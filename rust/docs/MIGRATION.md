# Migrating from v1 (Node.js) to v2 (Rust)

v2 is a drop-in replacement for the v1 CLI: same commands, same flags, same
aliases, same files. There is no data migration step.

## What changes

| | v1 | v2 |
| --- | --- | --- |
| Runtime | Node.js 18+ | none (single binary) |
| Install | `npm install -g ace-id` | release binary on `PATH`, or `cargo build --release` |
| Version string | `ace-id 0.1.4` | `ace-id 2.0.0` |
| Release channel | npm | GitHub Releases (v2 tags) |
| Source location | `src/`, `bin/` on `master` | `rust/` on `v2-rust` |

## What does not change

- Command names, subcommands, flags, short flags and aliases.
- Usage/error strings and exit codes.
- `~/.config/aidc/tokens.json` (or `%APPDATA%\aidc\tokens.json`): same path, same
  JSON shape, same 0600 mode.
- `.aid.json` and `.aid-setup-recovery.json`: same keys and formats.
- Environment variables and their defaults.
- OAuth client id (`aidc-cli`), redirect URI (`http://127.0.0.1:<port>/callback`),
  endpoints (`/auth`, `/token`, `/me`, `/apps`, `/clients`) and PKCE parameters.

## Steps

1. Install the v2 binary (see the [README](../README.md#install)). Keep the npm
   package installed until you are happy.
2. Check the binary:

   ```sh
   aid --version   # ace-id 2.0.0
   aid whoami      # uses the existing v1 session, no re-login
   ```

   If `whoami` reports `Not signed in`, run `aid login` once — the session file was
   probably absent or expired, which v1 would also have required.
3. In a linked project, confirm v2 reads the same configuration:

   ```sh
   aid sync --dry-run
   ```

   It should print a plan (or `No metadata changes detected.`) and write nothing.
4. When satisfied, remove the npm package: `npm uninstall -g ace-id`.

Both CLIs may be used alternately: they share the token file, the lock file and
the project configuration. Concurrent use is safe (the refresh lock is shared).

## Behaviour differences (deliberate)

1. **`--help` and `--version` work with invalid configuration.** v1 evaluated
   `AIDC_ISSUER` at import time and failed even for `--version`; v2 validates it
   only in commands that need the issuer.
2. **A failed `login` prints its error once.** v1 printed the same message twice
   (once from the spinner, once from the top-level handler).
3. **Missing or non-string API fields are rendered as `—` in tables and omitted
   from key/value blocks** instead of `undefined` / `[object Object]`.
4. **Browser detection does not spawn a shell.** v1 ran
   `sh -c "command -v xdg-open"`; v2 scans `PATH` directly.
5. **`sync` and `clients list` read `/apps` and `/clients` sequentially** instead
   of concurrently, in v1's declaration order. Same output; v1's own order and
   error precedence are racy because it uses `Promise.all`.
6. **Proxy environment variables are honoured** (`HTTPS_PROXY`, `HTTP_PROXY`,
   `NO_PROXY`). v1 ignores them.
7. **The stored session records its issuer.** If `AIDC_ISSUER` changes, v2 refuses
   to use the old session instead of sending the token to the new host. `aid login`
   replaces it; v1 ignores the extra key.
8. **Redirects are refused with a specific message**
   (`API request was redirected unexpectedly; refusing to forward credentials.`)
   where v1 printed its generic `Network request failed…`.
9. **Timeout messages keep v1's number formatting** (`0.3s`, not `1s`) and JSON
   output keeps v1's key order, so scripts that diff output see no change.

None of these change the command surface or any file format.

## Rolling back

Remove the v2 binary and reinstall the npm package. Your tokens and `.aid.json`
are untouched by v2 (it only writes what v1 would write), so v1 keeps working
immediately.

## Versioning

v2 releases use their own tags (`v2.x.y`) and assets. They never overwrite v1
release artifacts and are never published to npm. v1 continues to be released
from `master`.
