# Ace ID CLI

**Ace ID CLI** is the terminal-first command-line interface for Ace Identity.

It manages Ace ID applications and OAuth clients, handles OIDC authentication, and helps configure projects without forcing a particular frontend framework or SDK release.

## Install

Requires **Node.js 18+**.

```sh
npm install -g ace-id
```

Run without installing globally:

```sh
npx ace-id --help
```

### Command aliases

All three names use the same CLI:

```text
aid
ace-id
aidc
```

`aid` is canonical. `ace-id` and `aidc` are compatibility aliases for scripts and existing projects. `apps` is the canonical application command; `app` remains a compatibility alias.

## Quick start

```sh
aid login
aid setup
```

`aid setup` is intentionally fast and uses sensible defaults. It does not force a wizard on you.

For a guided flow:

```sh
aid setup --interactive
```

The interactive wizard shows a seven-step progress display for project detection, authentication, options, remote application/client creation, SDK setup, and local config. It lets you review the project name, redirect URI, and OAuth scopes, and asks before replacing an existing `.aid.json`.

### Setup options

```sh
aid setup [--interactive] [--name <name>] [--redirect <uri>] [--scope <scope>...] [--force]
aid setup --recover
# In non-interactive environments, explicitly approve cleanup:
aid setup --recover --force
```

Examples:

```sh
aid setup
aid setup --interactive
aid setup --name "My App" --redirect http://localhost:3000/callback
aid setup --scope openid --scope profile --scope email
aid setup --recover
```

If setup is interrupted after creating remote resources, rerun `aid setup --recover` from the same project directory to retry safe cleanup. Interactive runs ask for confirmation before deleting remote resources; non-interactive runs require `--force`. The recovery journal is local, ignored by Git, and checked against `.aid.json` so completed resources are not deleted.

## Authentication

Login uses **OpenID Connect Authorization Code + PKCE (S256)**.

```sh
aid login
aid test-auth
aid whoami
aid logout
```

The CLI generates a fresh PKCE verifier/challenge and random state per login, binds the callback to `127.0.0.1`, and validates callback state. Tokens are stored in the platform config directory with restrictive permissions where supported. Expiring sessions refresh automatically, concurrent refresh requests are serialized across processes, and network requests have timeouts. Authenticated API requests are constrained to the configured issuer and reject unexpected redirects. Run `aid test-auth` to make an authenticated request to the identity endpoint and verify that the stored session still works; it never prints tokens.

Environment overrides:

```sh
AIDC_ISSUER=https://identity.ace-base.cc
AIDC_CLIENT_ID=aidc-cli
AIDC_PORT=7357
AIDC_SCOPES="openid profile email offline_access"
AIDC_CALLBACK_TIMEOUT_MS=300000
AIDC_REFRESH_LEEWAY_MS=60000
AIDC_REQUEST_TIMEOUT_MS=15000
```

`AIDC_ISSUER` must use HTTPS unless it points to localhost. A path prefix in the issuer URL is preserved for authentication and API endpoints.

## SDK compatibility

The CLI is **SDK-version agnostic**. It does not require the CLI version to match your Ace ID SDK version. Existing JavaScript SDK installations are detected by package name, regardless of their installed version.

For JavaScript projects, the CLI targets `ace-id-sdk` and does not pin the CLI to a specific JavaScript SDK release. For Android, automatic setup uses a pinned, hash-verified native SDK artifact. Your application owns its SDK dependency and its upgrade schedule.

## Project detection

Setup detects projects from the **current working directory only**. It does not guess from parent workspaces or unrelated folders.

Supported detection includes Next.js, React, Vite, vanilla JavaScript, Android Java, and Android Kotlin. Package-manager detection supports npm, pnpm, Yarn, and Bun.

The CLI is designed to work across Windows, macOS, Linux, WSL, and terminal environments such as Termux, provided Node.js 18+ is available. Help text, key-value output, and resource tables adapt to narrow terminals; long IDs are wrapped rather than hidden, and spinner colors follow the actual output stream. Project detection performs independent filesystem checks concurrently and avoids collecting every Android source path into memory.

## Multiple applications and smart command routing

A single CLI session can manage any number of Ace ID applications. Remote application management is account-wide, while each local project can link to the application it uses.

`apps` is canonical:

```sh
aid apps list
aid apps create --name "Web"
aid apps get <app-id>
aid apps update <app-id> --name "Web Production"
aid apps delete <app-id>
aid link <app-id>
aid sync
```

`app` is accepted for compatibility. Common application actions also route to the canonical `apps` command:

```sh
aid app list
aid list
aid create --name "Web"
aid get <app-id>
aid update <app-id> --name "Web Production"
aid delete <app-id>
```

For example, `aid delete <app-id>` executes as `aid apps delete <app-id>`. This is argument normalization, not shell-level text rewriting.

## Link and sync

Use `link` to bind an existing Ace ID application to the current folder and create `.aid.json`:

```sh
aid link <app-id>
aid link <app-id> --client <client-id>
```

If `.aid.json` already exists, linking refuses to overwrite it unless you explicitly use `--force`.

`sync` refreshes the linked application's and OAuth client's metadata from the issuer, updates the local redirect URI/scopes when a client is linked, and preserves unrelated project settings. It validates the issuer and remote response before writing the config atomically. Passing an application ID switches the current folder to that application:

```sh
aid sync
aid sync <app-id>
```

`link` and `sync` do not silently create remote applications or clients. They validate/read existing resources and maintain the local link.

## Commands

```sh
aid setup
aid login
aid test-auth
aid logout
aid whoami

aid apps list
aid apps create --name "My App"
aid apps delete <app-id>

aid clients list
aid clients create --name "Web App" --redirect https://example.com/callback
aid clients delete <client-id>

aid version
aid help
```

## Configuration

Project configuration is stored in `.aid.json`. Do not commit it if your workflow treats its identifiers or local configuration as sensitive. The CLI writes it with restrictive permissions where the operating system supports them. User authentication data is stored separately.

## Security model

The CLI uses PKCE, one-time callback state validation, loopback-only callback binding, issuer-origin and path checks, redirect rejection for authenticated API/token requests, bounded network operations, atomic local token/config writes, cross-process session locking, strict token-shape validation, restrictive session-file permissions, argument-array process execution, bounded/redacted API errors, and pinned/hash-verified Android artifacts.

## Troubleshooting

If authentication is stale:

```sh
aid logout
aid login
```

If a project needs to be reconfigured:

```sh
aid setup --force
```

If browser launch is unavailable, `aid login` prints the authorization URL so it can be opened manually.

For CI or headless environments, use the supported non-interactive authentication flow provided by your Ace ID deployment.

## Development

```sh
npm install
npm run check
npm test
node bin/aid.js --help
node bin/aid.js --version
npm pack --dry-run
```

CI syntax-checks JavaScript files, runs regression tests, validates package generation, and smoke-tests CLI aliases.

## Changelog

See [CHANGELOG.md](./CHANGELOG.md).

## License

MIT
