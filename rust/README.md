# Ace ID CLI v2 (Rust)

A complete Rust implementation of the Ace ID CLI. It is a separate, independently
releasable program from the Node.js v1 CLI: same commands, same configuration
files, same OAuth flow, no Node.js runtime.

- **Binary:** `aid` (the `ace-id` and `aidc` aliases are accepted as a leading
  argument, exactly like v1).
- **Location:** all v2 code lives in `rust/` on the `v2-rust` branch. v1 (`src/`,
  `bin/`, `package.json`) is untouched and still released from `master`.
- **Version:** `ace-id 2.0.0`.

## Install

### Prebuilt binary (recommended)

Download the archive for your platform from the v2 GitHub Releases page, verify it
against `SHA256SUMS`, and put `aid` on your `PATH`:

```sh
# Linux x86_64 example
mkdir -p "$HOME/.local/bin"
curl -LO https://github.com/aceyash-dev/aid-cli/releases/download/v2.0.0/aid-x86_64-unknown-linux-gnu
curl -LO https://github.com/aceyash-dev/aid-cli/releases/download/v2.0.0/SHA256SUMS
sha256sum -c SHA256SUMS --ignore-missing
install -m 0755 aid-x86_64-unknown-linux-gnu "$HOME/.local/bin/aid"
export PATH="$HOME/.local/bin:$PATH"
aid --version
```

### From source

```sh
git clone https://github.com/aceyash-dev/aid-cli.git
cd aid-cli
git checkout v2-rust
cd rust

cargo build --release

mkdir -p "$HOME/.local/bin"
install -m 0755 target/release/aid "$HOME/.local/bin/aid"
export PATH="$HOME/.local/bin:$PATH"

aid --version
```

Requires a stable Rust toolchain (1.75 or newer). No Node.js is needed at build or
run time.

### Supported platforms

| Target | Status |
| --- | --- |
| `x86_64-unknown-linux-gnu` | Built, executed, tested and measured here |
| `aarch64-unknown-linux-gnu` | CI builds and runs it natively on `ubuntu-24.04-arm` |
| `x86_64-pc-windows-msvc` | CI builds and runs it natively on `windows-latest` |
| `aarch64-apple-darwin` | CI builds and runs it natively on `macos-latest` |
| `x86_64-apple-darwin` | CI builds and runs it natively on `macos-13` |
| Android / Termux | **Not verified.** The `termux-open-url` browser path is implemented; the platform has never been exercised |

A green CI build means the binary compiled *and executed* (`--version`, `--help`,
`logout`) on that platform. It does not prove the OAuth flow, file permissions or
browser hand-off work there — treat those as unverified until someone runs them on
the device. The Windows-specific code paths (`APPDATA`, `explorer.exe`, `tasklist`
liveness probe, `.cmd` package managers) are implemented but only covered by
compilation and the CI smoke test.

## Quick start

```sh
aid login          # OAuth 2.0 Authorization Code + PKCE (S256) in your browser
aid setup          # create the app + OAuth client, install the SDK, write .aid.json
aid whoami         # confirm the session works
aid sync --dry-run # preview metadata changes for this folder
```

`aid setup` is deliberately fast and non-interactive by default. Use
`aid setup --interactive` for the guided wizard (requires a TTY).

## Command reference

Full details: [`docs/COMMANDS.md`](docs/COMMANDS.md).

```text
aid setup [--interactive] [--name <name>] [--redirect <uri>] [--scope <s>...] [--force] [--recover]
aid login
aid logout
aid whoami [--json]
aid test-auth [--json]
aid apps list [--search <text>] [--limit <n>] [--json]
aid apps create --name <name> [--description <text>]
aid apps get <id> [--json]
aid apps update <id> [--name <name>] [--description <text>]
aid apps delete <id> [<id> ...]
aid clients list [--search <text>] [--limit <n>] [--json]
aid clients create --name <name> --redirect <uri> [--redirect <uri>...] [--scope <s>...] [--id <id>] [--public]
aid clients get <client_id> [--json]
aid clients delete <client_id>
aid clients rotate-secret <client_id>
aid link <app_id> [--client <client_id>] [--force]
aid sync [<app_id>] [--dry-run]
aid version
aid help
```

Compatibility aliases: `init` → `setup`, `app` → `apps`, and the bare
`list` / `create` / `get` / `update` / `delete` shorthands that imply `apps`.
Commands can be chained with `;` (`aid login \; aid apps list`); the chain stops
at the first failure.

## Authentication

OIDC Authorization Code flow with PKCE (S256), a loopback redirect on
`http://127.0.0.1:7357/callback`, state validation and single-use callbacks.
Tokens are stored in the same place and format as v1, so an existing v1 session
keeps working. See [`docs/AUTHENTICATION.md`](docs/AUTHENTICATION.md).

## Configuration

Environment variables (`AIDC_ISSUER`, `AIDC_CLIENT_ID`, `AIDC_PORT`,
`AIDC_SCOPES`, `AIDC_CALLBACK_TIMEOUT_MS`, `AIDC_REFRESH_LEEWAY_MS`,
`AIDC_REQUEST_TIMEOUT_MS`, `BROWSER`, `NO_COLOR`) behave exactly as in v1.
v2 additionally honours `HTTPS_PROXY`/`HTTP_PROXY`/`NO_PROXY` (v1 does not) and
accepts `AIDC_SDK_RELEASE_BASE` to install the Android SDK from a mirror.
See [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md).

## Migrating from v1

v1 and v2 can coexist: they read and write the same `tokens.json` and
`.aid.json`. Install the v2 binary, keep using the same commands, and remove the
npm package when you are satisfied. Details, including the two deliberate
behaviour changes, are in [`docs/MIGRATION.md`](docs/MIGRATION.md).

## Verified behaviour

`rust/REVIEW.md` records what was measured against the real v1 CLI and the real
services: 81 differential cases (byte-identical output, request traces and config
files), 12 v1⇄v2 interoperability checks including a simultaneous refresh race,
live SDK download and `npm install`, and a fresh install/uninstall run.

## Feature parity

Every v1 command, flag, alias, file format and message is ported. The
line-by-line matrix — with the test that covers each row — is in
[`PARITY.md`](PARITY.md). The v1 audit (including the bugs found and deliberately
not reproduced) is in [`AUDIT.md`](AUDIT.md).

## Development

```sh
cd rust
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for the module layout, test
strategy and contribution rules.

## Security

Security notes and the threat model: [`docs/SECURITY.md`](docs/SECURITY.md).
Please report vulnerabilities privately rather than in a public issue.

## Troubleshooting

[`docs/TROUBLESHOOTING.md`](docs/TROUBLESHOOTING.md).

## Changelog

[`CHANGELOG.md`](CHANGELOG.md).

## License

MIT — see [`../LICENSE`](../LICENSE).
