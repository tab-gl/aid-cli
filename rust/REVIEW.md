# Hardening review — round 2

Response to the pre-merge review. Everything below was measured against the real
code on `v2-rust`, not restated from the earlier report. Where something is still
unverified, it says so.

Tooling added for this round (all in `audit/`, all runnable, none of them part of
the shipped binary):

| Script | What it does |
| --- | --- |
| `audit/diff_harness.py` | Runs the **real v1 (Node) CLI** and the **real v2 (Rust) binary** against one mock API with identical inputs and compares stdout, stderr, exit code, resulting `.aid.json`/`tokens.json` and the HTTP request trace. 81 cases. |
| `audit/interop.py` | v1 ⇄ v2 session interoperability, including a **simultaneous refresh race** between the two CLIs. 12 checks. |
| `audit/live_sdk.py` | Real GitHub SDK artifact download + real `npm install` of the JS SDK, for both CLIs. |
| `audit/fresh_install.sh` | Clone → build → install onto `PATH` in an empty `HOME` → run → uninstall. |

## 1. Feature parity — measured, not asserted

`python3 audit/diff_harness.py` → **81 cases, 81 identical** (after the fixes
below). Two entries carry a documented allowance:

- `err-redirect`: v1 reports the generic `Network request failed…` because its
  `/redirect/i` branch is unreachable; v2 names the cause. Output is otherwise
  identical (exit code, no credential forwarded).
- `err-clients-429` / `clients list` / `sync`: v1 issues two reads in one
  `Promise.all`, so its request order and error precedence are racy. v2 pins the
  declaration order, which makes it deterministic; the request trace comparison
  for those cases is order-insensitive, and on failure v2 sends one request fewer.

### Divergences the harness found and that are now fixed

| # | Divergence | Fix |
| --- | --- | --- |
| 1 | `--json` output key order was alphabetical in v2 (`description, id, name`) instead of v1's insertion order (`id, name, description`) | enabled `serde_json`'s `preserve_order` |
| 2 | `.aid.json` was rewritten in alphabetical key order by `setup`/`sync`/`link` — a silent, visible change to the user's file | same fix; existing key order is now preserved and new keys are appended, exactly like v1 |
| 3 | Request bodies were serialised alphabetically (`{"name","public","redirect_uris","scopes"}`) instead of v1's `{"name","redirect_uris","scopes","public"}` | same fix |
| 4 | `help` printed one blank line between blocks; v1 prints two | added the missing blank line before every block |
| 5 | Timeout messages rounded up (`Request timed out after 1s.` for a 300 ms deadline); v1 prints `0.3s` | `util::js_seconds` reproduces `ms / 1000` formatting; also used for the callback timeout |
| 6 | `clients list` and `sync` read order differed from v1's declaration order | pinned to v1's order |

Byte-identical output now covers: `help`, `--version`, every usage error, table
and field layout, `apps`/`clients` CRUD, `whoami`/`test-auth` (plain and JSON),
`link`, `sync` (plan, dry-run, apply, no-op), `setup` (fast path, existing config,
drift, flags, recovery), logout, chaining, and the error paths (400/401/404/429,
HTML bodies, invalid JSON, timeouts).

## 2. Dependency and size audit

| Metric | Value |
| --- | --- |
| Direct dependencies | 7 (`serde`, `serde_json`, `sha2`, `getrandom`, `ureq`, `url`, `terminal_size`) + `libc` (unix only) |
| Crates in `Cargo.lock` | 90 (includes build-time `syn`/`quote`/`proc-macro2` and Windows-only `windows-sys`) |
| HTTP clients | 1 (`ureq`) |
| TLS stacks | 1 (`rustls` + `ring`) |
| Async runtimes | 0 (no Tokio, no `async-std`) |
| Duplicate crate versions | `webpki-roots` 0.26 → 1.0 (a compatibility shim inside ureq's own tree, not a second TLS stack) and two `windows-sys` versions (platform-only, never compiled on Linux) |
| Release binary | 1,973,152 bytes (1.88 MiB), `opt-level="z"`, LTO, `codegen-units=1`, stripped |
| Startup (`--version`, 20 runs) | median 10.8 ms, min 8.4 ms |

`preserve_order` added `indexmap` (+ `equivalent`, `hashbrown`) and ~9 KB of binary;
that is the cost of byte-identical JSON, and it also stops v2 from reordering the
user's `.aid.json`.

Rejected on purpose: `clap`, `tokio`, `thiserror`, `dirs`, `tempfile`,
`dialoguer`, `oauth2`, `regex` (redaction is hand-written and unit-tested).

## 3. Token compatibility — including a live v1/v2 race

`python3 audit/interop.py` → **12/12 checks**:

- v2 `login` writes `tokens.json` with mode `0600`; **v1 reads it** and reports the
  account; **v1 `logout` deletes it**.
- An expired session is refreshed **exactly once** when v1 and v2 start
  simultaneously against the same token file (both exit 0, both use the new token).
- A v1-shaped session (no `issuer` key) is accepted by v2 unchanged.

New in this round: **sessions are pinned to their issuer**. v2 writes
`"issuer": "<AIDC_ISSUER>"` into `tokens.json`; if the stored session belongs to a
different issuer, v2 refuses to use it instead of sending the token to the wrong
service:

```text
✗ The stored session belongs to https://other.example. Run `aid login` to sign in to https://identity.ace-base.cc.
```

Sessions written by v1 have no `issuer` key and keep working (verified above), so
the shared store stays compatible. `login` always replaces the session, which is
the explicit way to switch issuers.

## 4. Cross-platform — corrected claims and stronger CI

Facts in this environment: only `x86_64-unknown-linux-gnu` was built, executed and
measured; the sandbox has no other Rust target std installed, so even
`cargo check --target x86_64-pc-windows-msvc` is impossible here. Nothing else is
claimed as verified.

What changed:

- The README no longer implies cross-compilation equals verification.
- CI now **builds and executes** the binary natively on every platform it
  publishes: `ubuntu-latest` (x86_64 Linux), `ubuntu-24.04-arm` (aarch64 Linux),
  `windows-latest`, `macos-latest` (arm64), `macos-13` (x86_64). Each job runs
  `--version`, `--help` and a filesystem-touching `logout` before uploading.
- **Termux/Android remains unverified.** The Termux-specific code path
  (`termux-open-url`) is implemented but has never been executed on a device; the
  config directory, file modes and browser hand-off there are untested.

## 5. Real SDK paths — verified against the live services

`python3 audit/live_sdk.py`:

- The pinned Android artifact was fetched from the real release
  (`github.com/tab-gl/aid-sdk`, tag `1.1`): **108,945 bytes, SHA-256
  `8fde79fd…dba2`, matching the pinned constant in both v1 and v2.**
- v2's full path — release metadata → asset URL → 302 → bytes → digest check →
  SHA-256 check → `libs/library-release.aar` → `sdk_path` recorded — was executed
  with those real bytes and passed.
- The real JavaScript SDK install was executed: `npm install ace-id-sdk` inside a
  Next.js fixture installed **ace-id-sdk 0.2.3**, recorded it in `package.json`, and
  both CLIs reported `installed`.

Two real findings from doing this:

1. **The SDK repository was renamed** (`aceyash-dev/ace-id-sdk` → `tab-gl/aid-sdk`).
   The GitHub API answers with a 301. v1 followed it implicitly; v2 rejected
   redirects on that request and would have failed the Android install. Fixed:
   unauthenticated downloads may follow up to 5 redirects (`Request::follow_redirects`),
   while every credentialed request (API, token endpoint) still refuses redirects.
   Asset downloads also redirect to a CDN, which v2 could not have completed before
   this fix. The pinned SHA-256 remains the integrity control.
2. **Neither CLI honours proxy environment variables out of the box** (undici does
   not read them). v2 now reads `HTTPS_PROXY`/`HTTP_PROXY` (and lowercase forms),
   honours `NO_PROXY`, and never proxies loopback targets. v1 is unchanged.

Also added: `AIDC_SDK_RELEASE_BASE` so the release endpoint can point at a mirror
(air-gapped installs, and the test above). The digest pin is still enforced.

## 6. Fresh install / uninstall

`bash audit/fresh_install.sh`: clones the branch, builds from source, installs into
an empty `~/.local/bin`, runs `--version`, `--help`, `logout`, `whoami` (unauthenticated)
and an unknown command from a clean `HOME`, then uninstalls and confirms the binary
is gone. Passes.

## 7. Security items from the review

- **Terminal control characters in untrusted error text** — fixed. `sanitize_detail`
  now drops C0 controls (except `\r\n\t`, which become spaces), DEL and C1 controls,
  so an issuer cannot emit cursor moves, title changes or hyperlinks. Unit-tested.
- **Token store per issuer** — see §3: the store stays shared (v1 compatibility) but
  the session is pinned to its issuer.
- **Simultaneous v1/v2 refresh** — see §3: exactly one refresh, both processes succeed.
- Residual risks that are *not* fixed and are documented instead:
  - TLS trust uses the bundled `webpki-roots`. A corporate TLS-inspecting proxy
    (like the one in this sandbox) will fail with `tls connection init failed`;
    there is no `AIDC_CA_BUNDLE` yet. Workaround: download the artifact manually and
    place it in `libs/` — `setup`'s inspection verifies the hash.
  - The lock file remains advisory (mitigated by re-reading the token file after
    acquiring it).

## 8. Verification summary for this round

```text
cargo fmt --check                                        pass
cargo clippy --all-targets --all-features -- -D warnings  pass, no warnings
cargo test                                               191 tests, 0 failures
                                                         (123 unit + 68 integration)
cargo build --release                                    pass, 1,973,152 bytes
audit/diff_harness.py                                    81/81 cases identical
audit/interop.py                                         12/12 checks
audit/live_sdk.py                                        5/5 Android + 6/6 JS checks
audit/fresh_install.sh                                   pass (install + run + uninstall)
v1 regression suite (node --test)                        62 tests, 0 failures
```

Still not verified, and not claimed: Termux/Android runtime, macOS/Windows runtime
behaviour on real hardware (CI executes the binaries, which is not the same as
exercising OAuth, permissions and browser launch there), interactive wizard
prompts, and enterprise TLS interception.
