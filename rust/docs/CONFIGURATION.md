# Configuration

## Environment variables

Identical names, defaults and validation as v1.

| Variable | Default | Rules |
| --- | --- | --- |
| `AIDC_ISSUER` | `https://identity.ace-base.cc` | Must be an absolute URL. HTTPS required unless the host is `localhost`, `127.0.0.1` or `[::1]`. No credentials, query string or fragment. Trailing slashes are stripped. |
| `AIDC_CLIENT_ID` | `aidc-cli` | Trimmed; empty means the default. |
| `AIDC_PORT` | `7357` | Integer 1–65535. Also determines `redirect_uri`. |
| `AIDC_SCOPES` | `openid profile email offline_access` | Space-separated, non-empty. |
| `AIDC_CALLBACK_TIMEOUT_MS` | `300000` (5 min) | Positive integer. |
| `AIDC_REFRESH_LEEWAY_MS` | `60000` | Positive integer. |
| `AIDC_REQUEST_TIMEOUT_MS` | `15000` | Positive integer; applies to API calls, token endpoints and SDK downloads, including body reads. |
| `BROWSER` | unset | Command used to open the authorize URL. |
| `NO_COLOR` | unset | Any value disables colour. |
| `TERM=dumb` | — | Disables colour. |
| `XDG_CONFIG_HOME` | `~/.config` | Base for `aidc/tokens.json` (POSIX). |
| `APPDATA` | `%USERPROFILE%` | Base for `aidc\tokens.json` (Windows). |

Validation errors are reported before any network activity, e.g.:

```text
✗ AIDC_ISSUER must use HTTPS unless it targets localhost.
✗ AIDC_PORT must be an integer between 1 and 65535.
✗ Timeout and refresh-leeway environment values must be positive integers.
```

## Token file

See [`AUTHENTICATION.md`](AUTHENTICATION.md#stored-session). Location, mode and
JSON shape are shared with v1, so both CLIs can use the same session.

## Project configuration (`.aid.json`)

Written by `setup` and `link` in the current directory; mode `0600`; two-space
JSON with a trailing newline.

```json
{
  "issuer": "https://identity.ace-base.cc",
  "app_id": "app_123",
  "client_id": "client_456",
  "redirect_uri": "http://localhost:3000/callback",
  "scopes": ["openid", "profile", "email", "offline_access"],
  "project_type": "next",
  "framework": "Next.js",
  "sdk": "ace-id-sdk",
  "sdk_path": "libs/library-release.aar",
  "sdk_version": "1.2.3",
  "app_name": "My App",
  "app_description": "…",
  "synced_at": "2026-11-12T09:16:22.737Z"
}
```

| Key | Owner | Notes |
| --- | --- | --- |
| `issuer`, `app_id`, `client_id` | `setup` / `link` | `sync` refuses a project whose issuer differs from the active one |
| `redirect_uri`, `scopes`, `app_name`, `app_description` | `sync` | Only these four are rewritten by `sync` |
| `project_type`, `framework`, `sdk`, `sdk_path`, `sdk_version` | `setup` / `link` | Refreshed by relinking, not by `sync` |
| anything else | you | Preserved verbatim by `sync` and by same-app relinks |

`.aid.json` is ignored by git in this repository (see `.gitignore`).

## Recovery journal (`.aid-setup-recovery.json`)

```json
{
  "issuer": "https://identity.ace-base.cc",
  "app_id": "app_123",
  "client_id": null,
  "created_at": "2026-11-12T09:16:22.737Z",
  "stage": "app-created"
}
```

`stage` is one of `app-created`, `resources-created`, `cleanup-required`. While it
exists, `aid setup` refuses to start; `aid setup --recover` either removes the
leftover resources or, when `.aid.json` already references them, just clears the
journal.

## SDK behaviour

| Project | SDK mode | What `setup` does |
| --- | --- | --- |
| Next.js, Vite, React | `build` | Skips the install when `ace-id-sdk` is already declared in `package.json`; otherwise runs `npm install` / `pnpm add` / `yarn add` / `bun add ace-id-sdk`. |
| Vanilla JavaScript | `manual` | Reports `not applicable`; the SDK is wired by hand. |
| Android (Kotlin/Java) | `android` | Downloads `library-release.aar` from the pinned `aceyash-dev/ace-id-sdk` release `1.1`, checks the release digest and the SHA-256 of the bytes, and writes `libs/library-release.aar` (mode 0644). |

A failed SDK step is reported (`SDK setup skipped: …` / `SDK status: install failed`)
and does not abort the rest of setup, matching v1.
