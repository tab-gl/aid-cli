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

The interactive wizard lets you review:

- Project name
- Redirect URI
- OAuth scopes

It also asks before replacing an existing `.aid.json`.

### Setup options

```sh
aid setup [--interactive] [--name <name>] [--redirect <uri>] [--scope <scope>...] [--force]
```

Examples:

```sh
aid setup
aid setup --interactive
aid setup --name "My App" --redirect http://localhost:3000/callback
aid setup --scope openid --scope profile --scope email
```

## Authentication

Login uses **OpenID Connect Authorization Code + PKCE (S256)**.

```sh
aid login
aid whoami
aid logout
```

The CLI:

- Generates a fresh PKCE verifier/challenge for every login.
- Uses a cryptographically random state value and validates it on callback.
- Binds the local callback server to `127.0.0.1`.
- Stores tokens in the platform configuration directory with restrictive permissions where supported.
- Refreshes expiring sessions automatically.
- Deduplicates concurrent refresh requests so multiple CLI operations do not race to rotate a refresh token.
- Clears invalid sessions after rejected refresh/API authentication.
- Applies request timeouts so a dead network cannot leave the CLI hanging forever.
- Never sends bearer tokens to a different origin than the configured Ace ID issuer.

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

`AIDC_ISSUER` must use HTTPS unless it points to localhost.

## SDK compatibility

The CLI is **SDK-version agnostic**.

It does not require the CLI version to match your Ace ID SDK version. Existing SDK installations are detected by package name, regardless of their installed version.

For JavaScript projects, the CLI targets the package:

```text
ace-id-sdk
```

It does not pin the CLI to a specific JavaScript SDK release.

For Android, the CLI uses the verified native SDK artifact for its automatic Android setup path. This is separate from JavaScript package-version compatibility.

The CLI itself does not bundle an SDK runtime. Your application owns its SDK dependency and its upgrade schedule.

## Project detection

Setup detects projects from the **current working directory only**. It does not guess from parent workspaces or unrelated folders.

Supported detection includes:

- Next.js
- React
- Vite
- Vanilla JavaScript
- Android Java
- Android Kotlin

Package-manager detection supports:

- npm
- pnpm
- Yarn
- Bun

The CLI is designed to work across Windows, macOS, Linux, WSL, and terminal environments such as Termux, provided Node.js 18+ is available.

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

`app` is accepted for compatibility:

```sh
aid app list
```

Common application actions also have smart shorthand routing. These are normalized to the canonical `apps` command:

```sh
aid list
aid create --name "Web"
aid get <app-id>
aid update <app-id> --name "Web Production"
aid delete <app-id>
```

For example, `aid delete <app-id>` executes as `aid apps delete <app-id>`. This is command normalization, not shell-level text rewriting, so it works consistently across terminals.

## Link and sync

Use `link` to bind an existing Ace ID application to the current local folder and create `.aid.json`:

```sh
aid link <app-id>
```

Optionally record a specific OAuth client:

```sh
aid link <app-id> --client <client-id>
```

If `.aid.json` already exists, linking refuses to overwrite it unless you explicitly use:

```sh
aid link <app-id> --force
```

`sync` refreshes the application recorded in the current folder:

```sh
aid sync
```

You can also switch the current folder to another application:

```sh
aid sync <app-id>
```

`link` and `sync` do not silently create remote applications or clients. They only validate/read the existing application and maintain the local link.

## Commands

```sh
aid setup
aid login
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

Project configuration is stored in:

```text
.aid.json
```

Do not commit this file if your workflow treats its identifiers or local configuration as sensitive. The CLI writes it with restrictive permissions where the operating system supports them.

User authentication data is stored separately from project configuration.

## Security model

The CLI is designed around a few simple rules:

1. **PKCE for browser login**
2. **State validation for callback CSRF protection**
3. **Loopback-only callback binding**
4. **Origin validation for API requests**
5. **Bounded network requests**
6. **Atomic local token/config writes**
7. **Strict token-shape validation**
8. **No shell interpolation for package installation**
9. **Pinned and hash-verified Android artifacts**
10. **No SDK version coupling for JavaScript projects**

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

For CI or headless environments, authenticate using the supported non-interactive authentication flow provided by your Ace ID deployment rather than trying to fake a browser session.

## Development

```sh
npm install
node bin/aid.js --help
npm pack --dry-run
```

CI also syntax-checks every JavaScript file under `bin/` and `src/`.

## Changelog

See [CHANGELOG.md](./CHANGELOG.md).

## License

MIT
