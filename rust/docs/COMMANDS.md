# Command reference

All commands accept the `--json` flag only where documented. Anything else is a
usage error, and every handled failure exits with status 1.

Global forms:

```text
aid <command> [options]
aid <command> [options] \; aid <command> [options]
```

- `aid`, `ace-id` and `aidc` are interchangeable. The alias may also appear as the
  first argument (`aid ace-id login` works).
- `;`, `;;` and `\;` separate chained commands. Chaining stops at the first
  failure.
- `help`, `h`, `--help`, `-h` print the help screen; `version`, `-v`,
  `--version` print `ace-id <version>`.

Aliases: `init` = `setup`, `app` = `apps`. The bare verbs `list`, `create`, `get`,
`update`, `delete` are shorthands for `apps <verb>` (so `aid create app -n x` is
`aid apps create -n x`).

---

## setup

```text
aid setup [--interactive] [--name <name>] [--redirect <uri>] [--scope <scope>...] [--force]
aid setup --recover [--force]
```

| Flag | Short | Meaning |
| --- | --- | --- |
| `--interactive` | `-i` | Guided wizard (7 steps, prompts for name/redirect/scopes). Requires a TTY. |
| `--wizard` | `-w` | Alias for `--interactive`. |
| `--name` | `-n` | Application name. Default: `package.json` name, else the folder name. |
| `--redirect` | `-r` | Redirect URI. Default `http://localhost:3000/callback`. |
| `--scope` | `-s` | Repeatable. Default: `AIDC_SCOPES` (`openid profile email offline_access`). |
| `--force` | `-f` | Replace an existing `.aid.json` and create new remote resources. |
| `--recover` | — | Clean up an interrupted setup using `.aid-setup-recovery.json`. |

Behaviour:

1. Refuses to start while a recovery journal exists.
2. If `.aid.json` exists and `--force` is absent, inspects the project and reports
   drift (project type, framework, SDK range/version, issuer, Android artifact)
   with minimal repair suggestions. Non-interactive runs then stop without
   creating anything.
3. Detects the project type (Next.js, Vite, React, vanilla, Android Kotlin/Java).
4. Requires a stored session; prompts for login when interactive, otherwise fails
   with `Not signed in. Run \`aid login\` first.`
5. Creates the application (`POST /apps`), writes the recovery journal, creates a
   public OAuth client (`POST /clients`), then installs the SDK.
6. Writes `.aid.json` (mode 0600) and clears the journal. Any failure after remote
   resources exist triggers cleanup of what was created.

## login

```text
aid login
```

Runs OAuth 2.0 Authorization Code with PKCE (S256): binds `127.0.0.1:<AIDC_PORT>`,
prints the authorize URL, opens a browser, validates `state`, exchanges the code,
and stores tokens. No flags.

## logout

```text
aid logout
```

Deletes the stored token file. Remote sessions are unaffected.

## whoami

```text
aid whoami [--json]
```

Calls `GET /me` and shows name, email and subject (falls back through
`name → preferred_username → email → sub`).

## test-auth

```text
aid test-auth [--json]
```

Verifies the stored session against `GET /me` and reports issuer, client id,
account and subject. JSON output includes `status: "ok"`.

## apps

```text
aid apps list   [--search <text>] [--limit <n>] [--json]
aid apps create --name <name> [--description <text>]
aid apps get    <id> [--json]
aid apps update <id> [--name <name>] [--description <text>]
aid apps delete <id> [<id> ...]
```

- `list` prints an `ID`/`NAME` table; `--search` matches id, name and description
  case-insensitively; `--limit` must be a positive integer.
- `create` requires `--name` (`-n`); `--description` (`-d`) is optional.
- `update` requires at least one of `--name`/`--description`.
- `delete` deletes each id in order and stops at the first failure; a 404 reports
  `No app found with ID <id>`.

## clients

```text
aid clients list   [--search <text>] [--limit <n>] [--json]
aid clients create --name <name> --redirect <uri> [--redirect <uri>...] [--scope <s>...] [--id <id>] [--public]
aid clients get    <client_id> [--json]
aid clients delete <client_id>
aid clients rotate-secret <client_id>
```

`list` joins `/clients` with `/apps` and the local `.aid.json` to report a
relationship for each client:

| Value | Meaning |
| --- | --- |
| `OK` | Client has an owner application and matches the local link |
| `MISLINKED` | The linked client's `app_id` differs from the local `app_id` |
| `ORPHANED: no app` | The client has no `app_id` |
| `ORPHANED: app missing` | `app_id` set but the application no longer exists |
| `REDIRECT MISMATCH` | The linked client's redirect URIs exclude the local `redirect_uri` |

`create` prints the client secret once (confidential clients); `rotate-secret`
prints the new secret and warns that the previous one is already invalid.

## link

```text
aid link <app_id> [--client <client_id>] [--force]
```

Validates the application (and the client, when given) against the API, then writes
`.aid.json` with the issuer, app id, optional client id and detected project
metadata.

- Without `--force`, an existing `.aid.json` is an error.
- Relinking the **same** app preserves its `client_id` and other app settings;
  switching to a **different** app drops them so the old app's client is never
  reused silently.
- A client that belongs to another application is rejected and nothing is written.

## sync

```text
aid sync [<app_id>] [--dry-run]
```

Reconciles `.aid.json` with the server:

- `--dry-run` prints the plan and writes nothing.
- Without changes it reports `No metadata changes detected.`
- With changes it rewrites only `app_name`, `app_description`, `redirect_uri` and
  `scopes`, sets `synced_at`, and leaves every other key untouched.
- If `.aid.json` changed while the remote reads were in flight, the write is
  abandoned and the command fails.
- `sync <app_id>` replaces the current link (equivalent to `aid link <app_id> --force`).
- Refuses projects linked to a different issuer.

## Exit codes and streams

| Situation | Exit | Stream |
| --- | --- | --- |
| Success | 0 | stdout |
| Handled failure | 1 | stderr (`✗ message`) |
| Warnings | unchanged | stderr (`! message`) |
