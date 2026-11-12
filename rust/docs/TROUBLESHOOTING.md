# Troubleshooting

## `Not signed in. Run \`aid login\` first.`

No usable token file. Run `aid login`. If you just logged in, check that
`XDG_CONFIG_HOME` (or `APPDATA` on Windows) is the same in both shells — the token
file lives under `<that base>/aidc/tokens.json`.

## `Stored Ace ID session is invalid or unsafe. Run \`aid login\` again.`

The token file is a symlink, is not a regular file, is not valid JSON, or has
fields of the wrong type. Inspect it, delete it, and log in again:

```sh
rm ~/.config/aidc/tokens.json
aid login
```

## `Port 7357 is already in use. Close the conflicting process or set AIDC_PORT.`

Another process (often a second `aid login`) holds the callback port. Close it or
pick another port:

```sh
AIDC_PORT=7358 aid login
```

Remember that `AIDC_PORT` also changes the redirect URI that is sent to the
issuer.

## `Timed out after 300s waiting for the browser.`

The browser never reached `http://127.0.0.1:<port>/callback`. Common causes: the
sign-in tab was closed, a firewall blocks loopback, or you signed in on a
different machine (the callback must arrive on the machine running the CLI). For
remote shells, forward the port:

```sh
ssh -L 7357:127.0.0.1:7357 you@host
```

## `Request timed out after 15s.`

Raise the deadline if your network is slow:

```sh
AIDC_REQUEST_TIMEOUT_MS=30000 aid apps list
```

## `Network request failed. Check your connection and try again.`

DNS or TCP failure reaching `AIDC_ISSUER`. Check the value of `AIDC_ISSUER` and
your proxy settings.

## `API request was redirected unexpectedly; refusing to forward credentials.`

The API answered with a 3xx (usually an HTTP→HTTPS redirect or a captive portal).
Use the canonical HTTPS issuer directly; v2 never follows redirects with a token
attached.

## `Server rejected the access token. Run \`aid login\` again.`

The API returned 401 and the stored session was deleted. Log in again. If this
happens immediately after a successful login, check that `AIDC_ISSUER` and
`AIDC_CLIENT_ID` match the service you are talking to.

## `Session expired and no refresh token is stored.`

The issuer did not return a refresh token (the `offline_access` scope is usually
required). Log in again; if it recurs, confirm `AIDC_SCOPES` includes
`offline_access`.

## `The stored session belongs to <issuer>.`

`AIDC_ISSUER` now points at a different issuer than the stored session. Run
`aid login` to sign in to the new issuer (the old session is replaced), or set
`AIDC_ISSUER` back to the issuer the session belongs to.

## `tls connection init failed` / `SDK download request failed`

TLS trust uses the bundled Mozilla root store. Behind a TLS-inspecting proxy the
handshake cannot be verified. Options: use a network without interception, or for
the Android SDK download the artifact manually and place it at
`libs/library-release.aar` (setup verifies its SHA-256). API and token traffic is
unaffected if the issuer's certificate chains to a public root.

## Behind a proxy

v2 reads `HTTPS_PROXY`/`HTTP_PROXY` and `NO_PROXY`; loopback is never proxied, and
`NO_PROXY` accepts exact hosts, `.suffix` and bare-domain suffixes, `host:port` and `*`.

**Proxy authentication is not supported.** A proxy that requires credentials answers
407 and the request fails (`Request failed (407): Proxy Authentication Required`).
Use an unauthenticated proxy, or add the host to `NO_PROXY`.

## `Timed out waiting for the Ace ID session lock…`

Another `aid` process holds the lock and did not finish within 30 s. If no other
`aid` process is running, remove the stale lock:

```sh
rm ~/.config/aidc/tokens.json.lock
```

## `An interrupted setup recovery record exists. Run \`aid setup --recover\` …`

A previous `setup` stopped after creating remote resources. Review
`.aid-setup-recovery.json`, then:

```sh
aid setup --recover            # interactive confirmation
aid setup --recover --force    # non-interactive
```

If `.aid.json` already references the recorded app/client, recovery just clears the
journal and deletes nothing.

## `Configuration drift detected:` in `aid setup`

The project changed since it was configured (framework, SDK version, issuer, or a
missing Android artifact). Read the listed repair suggestions; typically
`aid link <app_id> --force` refreshes local metadata without creating new remote
resources, and `aid sync` refreshes remote metadata.

## `SDK setup skipped: …` / `SDK status: install failed`

The package manager install failed or the network was unavailable. Setup still
completes. Install manually with the package manager your project uses:

```sh
npm install ace-id-sdk      # or pnpm add / yarn add / bun add
```

## `.aid.json changed while sync was running. No local changes were written`

The file was edited between the remote reads and the write. Re-run `aid sync`.

## `The linked project uses a different issuer.`

`.aid.json` was written against another issuer than the active `AIDC_ISSUER`.
Set the matching issuer, or relink:

```sh
AIDC_ISSUER=https://identity.ace-base.cc aid link <app_id> --force
```

## Output looks broken in a script or CI

Colour and the spinner are disabled automatically when stdout is not a TTY. Force
it with `NO_COLOR=1` and `TERM=dumb`. Tables switch to a stacked layout when the
terminal is narrower than 72 columns; the width comes from the terminal itself
(falling back to 80 columns when there is no terminal).

## Reporting a bug

Include `aid --version`, your OS, the exact command, and the output with secrets
removed (v2 already redacts tokens, credentials and URLs in error text).
