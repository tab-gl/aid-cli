# Development

## Requirements

- Stable Rust (1.75+). No Node.js is needed for the Rust crate.
- `rustfmt` and `clippy` components.

## Layout

```text
rust/
  Cargo.toml
  src/
    main.rs          # thin binary wrapper around cli::run
    lib.rs           # library root (used by tests)
    error.rs         # AidError + bail!/fail! macros
    util.rs          # base64url, hex, ISO-8601, constant-time compare
    http.rs          # ureq wrapper: deadline, no redirects, bounded body
    cli/
      mod.rs         # chaining, aliases, help, dispatch
      args.rs        # strict argument parser (parseArgs-compatible)
      commands/      # one module per command
    auth/            # pkce, oauth (authorize/exchange/callback/browser), token, refresh
    api/             # client, url resolution, error rendering
    config/          # env config, platform paths, atomic storage, project files
    project/         # project detection
    sdk/             # install + integrity verification
    terminal/        # colours, layout blocks, prompts, spinner
  tests/
    support/mod.rs   # mock HTTP server, isolated env, CLI runner
    cli_surface.rs   # help/version/aliases/usage errors
    api_commands.rs  # commands against a mock API
    auth_flow.rs     # login/refresh/logout end to end
    setup_flow.rs    # setup, drift, recovery, cleanup
    security.rs      # config validation, unsafe files, redaction, timeouts
  docs/              # command, auth, config, migration, security, troubleshooting
  PARITY.md          # v1 → v2 matrix with the covering test
  AUDIT.md           # v1 audit findings
  REVIEW.md          # round-2 hardening: differential, interop and live results
audit/               # verification scripts (not shipped, not part of the build)
  diff_harness.py    # v1 vs v2 differential run over 81 cases
  interop.py         # v1 ⇄ v2 session interop + simultaneous refresh race
  live_sdk.py        # real GitHub artifact + real npm install
  fresh_install.sh   # clone → build → install → run → uninstall
```

Design rules:

- **No async runtime.** The CLI is a short-lived synchronous program; `ureq` +
  `std::net` cover every request and the loopback listener.
- **No CLI framework.** `cli/args.rs` reproduces `util.parseArgs` semantics so
  usage messages and error behaviour stay identical to v1, and the binary stays
  small.
- **Modules own one concern.** Avoid new abstractions unless a second caller needs
  them.

## Checks (required before a commit)

```sh
cd rust
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

CI (`.github/workflows/rust-v2.yml`) runs the same four checks plus cross-target
builds. The v1 workflow is separate and unaffected.

## Tests

- **Unit tests** live beside the code (`#[cfg(test)] mod tests`) and cover pure
  logic: PKCE vectors, config parsing, token validation, locking, URL containment,
  error redaction, wrapping/layout, detection, semver ranges, hashing.
- **Integration tests** run the real binary (`tests/support/mod.rs` starts a mock
  server on `127.0.0.1:0` and gives each test an isolated `HOME`/
  `XDG_CONFIG_HOME`). They never touch the real API or your real config.
- The login test drives the whole PKCE flow: it parses the authorize URL from the
  child's stdout, replays the browser against the loopback callback, and asserts
  the token file, mode and token request contents.

Guidelines:

- A new behaviour needs a test; a fixed bug needs a test that fails before the fix.
- Never assert on timing-sensitive output formatting; assert on structure and
  messages.
- Tests must not require network access beyond loopback.

## Adding a command

1. Add a module under `src/cli/commands/` exposing `run(args: &[String]) -> Result<()>`.
2. Register it in `cli::normalize` and `cli::run_command`.
3. Reuse `cli::args`, `terminal::*` and `api::*` rather than rolling your own.
4. Add the help entry in `cli::print_help` and `docs/COMMANDS.md`.
5. Add a unit test for parsing and an integration test for the flow.
6. Update `PARITY.md` if it maps to v1 behaviour.

## Verification scripts

These complement `cargo test` by testing the shipped binary against the shipped
v1 CLI and the real services. They need `node` (for v1), a release build
(`cargo build --release`) and network access for the live checks:

```sh
cd rust && cargo build --release && cd ..
python3 audit/diff_harness.py      # 81 cases, expect "identical: 81"
python3 audit/interop.py           # 12 checks
python3 audit/live_sdk.py          # Android artifact + npm install
bash   audit/fresh_install.sh      # install/uninstall from a clean clone
```

They are intentionally outside the build: `cargo test` must stay hermetic, and the
live checks talk to GitHub and the npm registry.

## Commit style

Small, reviewable commits scoped to one area (e.g. "auth: port PKCE and callback
server"). Never commit to `master`; v2 work happens on `v2-rust`.

## Dependencies

| Crate | Why |
| --- | --- |
| `serde`, `serde_json` | JSON for tokens, project config, API payloads |
| `ureq` (rustls) | synchronous HTTP with TLS, redirect control and timeouts; avoids an async runtime |
| `sha2` | PKCE S256 and artifact verification |
| `getrandom` | OS CSPRNG for verifier/state/temp names |
| `url` | RFC 3986 parsing and relative resolution (containment checks depend on it) |
| `terminal_size` | real terminal width for responsive tables |
| `libc` (unix only) | restore default `SIGPIPE` so `aid --help \| head` exits quietly |

No other runtime dependencies are used; see `AUDIT.md` for the reasoning behind
rejecting `clap`, `tokio`, `thiserror`, `dirs`, `tempfile` and `dialoguer`.
