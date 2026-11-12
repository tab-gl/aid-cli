# v1 → v2 feature parity matrix

Reference implementation: `master` (Node.js, package `ace-id` 0.1.4).
Port: `rust/` on branch `v2-rust` (Rust, binary `aid` 2.0.0).

Status legend:

- **Ported** — behaviour reproduced, including messages, defaults and file formats.
- **Tested** — covered by an automated test (unit or integration). The test name
  is listed.
- **Gap** — implemented but not covered by an automated test in this environment;
  the reason is stated.

## Differential verification against v1

`audit/diff_harness.py` runs the real v1 CLI and the real v2 binary against one
mock API with identical inputs and compares stdout, stderr, exit code, the
resulting `tokens.json` / `.aid.json` and the HTTP request trace.

| Run | Cases | Identical |
| --- | --- | --- |
| First run (before the fixes below) | 81 | 74 |
| After the output/ordering fixes | 81 | **81** |
| After adding malformed input, permission modes and empty arguments | 102 | **102** |

Divergences found and fixed: JSON/`.aid.json`/request-body key order, `help` blank
lines, timeout message formatting, the read order of `sync` / `clients list`, and
five empty-string argument cases where v1's truthiness checks produce a usage error.
Details and evidence: [`REVIEW.md`](REVIEW.md).

Two allowances are recorded rather than "fixed", because v1 is either racy or
vague there:

- `clients list` / `sync` issue two reads in one `Promise.all`, so v1's request
  order and error precedence are non-deterministic; v2 pins v1's declaration order.
- On a 3xx, v1 falls through to `Network request failed…` (its `/redirect/i` branch
  is unreachable); v2 names the cause. Exit code and credential handling match.

## Command surface

| Feature | v1 source | v2 implementation | Status | Test |
| --- | --- | --- | --- | --- |
| `aid` canonical name, `ace-id`/`aidc` aliases accepted as first token | `bin/aid.js` `stripCliPrefix` | `cli::split_command_chain`, `strip_cli_prefix` | Ported, Tested | `cli::tests::binary_aliases_are_stripped`, `cli_surface::binary_aliases_are_accepted_as_a_prefix` |
| Command chaining on `;`, `;;`, `\;`, stopping at first failure | `splitCommandChain` | `cli::split_command_chain` | Ported, Tested | `cli::tests::chains_split_on_separators`, `cli_surface::commands_chain_and_stop_at_the_first_failure` |
| `help`/`h`/`--help`/`-h` and `version`/`-v`/`--version` | `runCommand` | `cli::run_command` | Ported, Tested | `cli_surface::help_prints_the_command_reference`, `cli_surface::version_reports_the_crate_version` |
| `init` → `setup`, `app` → `apps`, bare `list/create/get/update/delete` → `apps` | `normalizeCommand` | `cli::normalize` | Ported, Tested | `cli::tests::compatibility_shorthands_normalise` |
| Unknown command error `Unknown command: x` (exit 1) | `runCommand` | `cli::run_command` | Ported, Tested | `cli_surface::unknown_commands_fail_with_a_useful_error` |
| Unknown subcommand errors for `apps`/`clients` | `commands/apps/index.js` | `commands::apps::run` | Ported, Tested | `cli_surface::unknown_subcommands_are_reported` |
| Per-command usage strings and exit codes | `parseArgs` catch blocks | `cli::args` + each command | Ported, Tested | `cli_surface::usage_errors_match_v1_text` |
| Banner (half-block wordmark + `CLI v<version>`) | `ui/banner.js` | `terminal::output::show_banner` | Ported | `cli_surface::help_prints_the_command_reference` (banner present) |
| Responsive tables, key/value fields, command lists, word wrap | `ui/format.js` | `terminal::output` | Ported, Tested | `terminal::output::tests::*` |
| Colour palette, `NO_COLOR`, `TERM=dumb`, TTY detection | `ui/colors.js` | `terminal::output::colors_enabled` | Ported | `cli_surface` runs with `NO_COLOR=1` throughout |
| Screen clear on help (no alternate buffer) | `ui/screen.js` | `terminal::output::clear_screen` | Ported | — (visual, TTY-only) |
| Spinner while waiting for the browser | `ui/spinner.js` | `terminal::spinner` | Ported, Tested | `terminal::spinner::tests::spinner_is_a_no_op_without_a_terminal` |

## Authentication

| Feature | v1 source | v2 implementation | Status | Test |
| --- | --- | --- | --- | --- |
| Authorization Code flow with PKCE S256 | `auth/login.js`, `auth/pkce.js` | `auth::oauth`, `auth::pkce` | Ported, Tested | `auth_flow::login_completes_the_pkce_flow_and_stores_tokens` |
| Verifier = 32 random bytes base64url (43 chars) | `pkce.js` | `pkce::generate_verifier` | Ported, Tested | `auth::pkce::tests::verifier_is_base64url_with_expected_length` |
| Challenge = SHA-256 base64url | `pkce.js` | `pkce::challenge_for` | Ported, Tested | `auth::pkce::tests::challenge_matches_rfc7636_appendix_b` |
| State = 16 random bytes base64url | `pkce.js` | `pkce::generate_state` | Ported, Tested | `auth::pkce::tests::state_is_22_characters` |
| Loopback callback bound to `127.0.0.1` before the URL is printed | `auth/callback.js` | `auth::oauth::CallbackServer::bind` | Ported, Tested | `auth_flow::login_completes_the_pkce_flow_and_stores_tokens` |
| State validation; forged callbacks do not consume the login | `callback.js` | `oauth::handle_connection` | Ported, Tested | `auth_flow::forged_callbacks_do_not_consume_the_login` |
| Callback validation (method, path, `error`, missing `code`) | `callback.js` | `oauth::handle_connection` | Ported | covered by `auth_flow` (state/error paths); other branches unit-level |
| Single-use callback + callback timeout | `callback.js` | `CallbackServer::wait_for_code` | Ported, Tested | `auth_flow::forged_callbacks_do_not_consume_the_login` |
| Port-in-use message `Port N is already in use…` | `callback.js` | `CallbackServer::bind` | Ported | — (needs two live listeners; message asserted by code review) |
| Browser hand-off: `BROWSER`, `open`, `explorer.exe`, `termux-open-url`, `xdg-open` | `login.js` `openBrowser` | `oauth::open_browser` | Ported | — (opens a browser; tests set `BROWSER=true`) |
| Token endpoint exchange, error mapping | `login.js` | `oauth::exchange_code` | Ported, Tested | `auth_flow::login_reports_token_endpoint_failures`, `auth_flow::login_rejects_a_token_response_without_an_access_token` |
| Token persistence: `~/.config/aidc/tokens.json` (XDG), `%APPDATA%\aidc` | `auth/tokens.js` | `auth::token`, `config::paths` | Ported, Tested | `auth_flow::login_completes_the_pkce_flow_and_stores_tokens`, `security::written_project_config_is_private` |
| Token file mode 0600, directory 0700 | `tokens.js` | `config::storage` | Ported, Tested | `config::storage::tests::written_files_are_private` |
| Symlinked/non-regular token files rejected | `tokens.js` `loadTokens` | `storage::read_to_string_checked` | Ported, Tested | `security::symlinked_token_files_are_refused` |
| Corrupt / wrong-typed token documents rejected | `validateTokens` | `Tokens::from_json` | Ported, Tested | `security::corrupt_token_files_are_refused`, `security::token_documents_with_wrong_types_are_refused` |
| Atomic token writes (temp + `rename`, `create_new`) | `saveTokensUnlocked` | `storage::write_atomic` | Ported, Tested | `config::storage::tests::write_atomic_replaces_content_and_leaves_no_temp_files` |
| Cross-process lock file with stale-lock recovery | `withTokenLock`, `removeStaleLock` | `auth::token::with_token_lock` | Ported, Tested | `auth::token::tests::lock_is_exclusive_and_released`, `auth_flow::concurrent_processes_share_a_single_refresh` |
| In-process refresh de-duplication | `refreshPromise` | `refresh::REFRESH_MUTEX` | Ported | exercised by `auth_flow::concurrent_processes_share_a_single_refresh` (cross-process) |
| Refresh leeway (default 60s) | `isExpiringSoon` | `token::is_expiring_soon` | Ported, Tested | `auth::token::tests::expiry_uses_leeway` |
| Refresh merges previous refresh token/scope/type | `refresh.js` | `oauth::merge_tokens` | Ported, Tested | `auth::oauth::tests::merged_tokens_keep_previous_refresh_token` |
| Invalid refresh session clears tokens | `refresh.js` | `refresh::get_valid_access_token` | Ported, Tested | `auth_flow::invalid_refresh_sessions_are_cleared` |
| Missing refresh token clears tokens with guidance | `refresh.js` | `refresh::get_valid_access_token` | Ported, Tested | `auth_flow::expired_session_without_refresh_token_is_cleared` |
| `logout` deletes the session | `commands/logout.js` | `commands::logout` | Ported, Tested | `auth_flow::logout_deletes_the_stored_session` |
| `whoami`, `test-auth` (+ `--json`) | `commands/whoami.js`, `test-auth.js` | `commands::whoami`, `commands::test_auth` | Ported, Tested | `api_commands::whoami_*`, `api_commands::test_auth_reports_issuer_and_client` |

## API client

| Feature | v1 source | v2 implementation | Status | Test |
| --- | --- | --- | --- | --- |
| Bearer token attached to every request | `api/client.js` | `api::client::api_fetch` | Ported, Tested | `api_commands::whoami_prints_the_signed_in_account` |
| URL resolution pinned to the issuer origin and base path | `api/url.js` | `api::url::resolve_api_url` | Ported, Tested | `api::url::tests::*` |
| Path traversal / absolute URL rejection | `api/url.js` | `api::url::resolve_api_url` | Ported, Tested | `api::url::tests::traversal_outside_the_issuer_is_rejected` |
| Redirects rejected (`refusing to forward credentials`) | `redirect: "error"` | `http::Request` (`redirects(0)`) | Ported, Tested | `api_commands::redirects_are_never_followed` |
| Request timeout including body reads | `AbortSignal.timeout` | `http::Request` timeout | Ported, Tested | `api_commands::request_timeouts_are_reported`, `security::slow_endpoints_time_out_without_hanging` |
| 401 clears the stored session | `apiFetch` | `api::client::api_fetch` | Ported, Tested | `auth_flow::rejected_access_tokens_clear_the_session` |
| Status labels, JSON detail extraction, HTML suppression | `api/errors.js` | `api::errors` | Ported, Tested | `api::errors::tests::*` |
| Secret/URL redaction in error details | `sanitizeDetail` | `api::errors::sanitize_detail` | Ported, Tested | `api::errors::tests::secrets_are_redacted`, `security::bearer_tokens_never_appear_in_error_output` |
| Bounded error bodies (4 KiB) and 240-char details | `readBoundedBody` | `api::errors` | Ported, Tested | `api::errors::tests::oversize_bodies_are_bounded` |
| Invalid JSON responses reported, not silently accepted | `apiJson` | `api::client::api_json` | Ported | covered indirectly by payload-shape tests |
| Parallel `/apps` + `/clients` read in `sync` and `clients list` | `Promise.all` | sequential reads in v1's declaration order | **Deliberate difference** | Byte-identical output (`diff_harness`); v1's own order is racy |
| JSON output and `.aid.json` key order | insertion order (`JSON.stringify`) | insertion order (`serde_json` `preserve_order`) | Ported, Tested | `diff_harness` cases `apps-list-json`, `sync-apply`, `setup-vanilla` |
| Timeout message formatting (`0.3s`, `15s`) | `${ms / 1000}s` | `util::js_seconds` | Ported, Tested | `util::tests::seconds_match_javascript_number_formatting` |
| Help screen spacing | two blank lines between blocks | same | Ported, Tested | `diff_harness` cases `help`, `help-flag`, `help-word`, `no-args` |
| Proxy environment variables | ignored | `HTTPS_PROXY`/`HTTP_PROXY`/`NO_PROXY` honoured | **Addition** | `http::tests::loopback_targets_bypass_the_proxy` |
| Session pinned to its issuer | no issuer recorded | `issuer` key + refusal | **Addition** | `security::sessions_are_pinned_to_their_issuer`, `interop.py` |

## Applications and OAuth clients

| Feature | v1 source | v2 implementation | Status | Test |
| --- | --- | --- | --- | --- |
| `apps list` with `--search`, `--limit`, `--json`, table output | `commands/apps/list.js` | `commands::apps::list` | Ported, Tested | `api_commands::apps_list_renders_a_table_and_supports_filters` |
| Payload validation (`unexpected application list`) | `list.js` | `commands::expect_object_array` | Ported, Tested | `api_commands::apps_list_rejects_unexpected_payloads` |
| `apps create --name/-n`, `--description/-d` | `create.js` | `commands::apps::create` | Ported, Tested | `api_commands::apps_create_sends_json_and_reports_the_id` |
| `apps get <id> [--json]` | `get.js` | `commands::apps::get` | Ported, Tested | `api_commands::apps_get_and_update_round_trip` |
| `apps update <id>` (PATCH, "Nothing to update") | `update.js` | `commands::apps::update` | Ported, Tested | `api_commands::apps_get_and_update_round_trip` |
| `apps delete <id>...` incl. 404 handling | `delete.js` | `commands::apps::delete` | Ported, Tested | `api_commands::apps_delete_handles_multiple_ids_and_404` |
| `clients list` with ownership + link diagnostics (`OK`, `MISLINKED`, `ORPHANED: …`, `REDIRECT MISMATCH`) | `commands/clients/list.js` | `commands::clients::list`, `relationship_status` | Ported, Tested | `api_commands::clients_list_reports_link_diagnostics`, `commands::clients::tests::status_*` |
| `clients create` (`--name`, `--redirect` ×N, `--scope` ×N, `--id`, `--public`), secret shown once | `create.js` | `commands::clients::create` | Ported, Tested | `api_commands::clients_create_prints_the_secret_once` |
| `clients get`, `clients delete`, `clients rotate-secret` | `get.js`, `delete.js`, `rotate.js` | `commands::clients::*` | Ported, Tested | `cli_surface::usage_errors_match_v1_text`, `commands::clients::tests::delete_and_rotate_require_ids` |
| `--limit` must be a positive integer | `list-options.js` | `list_options::parse_list_options` | Ported, Tested | `list_options::tests::rejects_invalid_limits` |
| Case-insensitive search over id/name/description (apps) and client fields (clients) | `filterResources` | `list_options::filter_resources` | Ported, Tested | `list_options::tests::filtering_is_case_insensitive_and_limited` |

## Project detection, setup, SDK, linking and sync

| Feature | v1 source | v2 implementation | Status | Test |
| --- | --- | --- | --- | --- |
| Detection scoped to the current directory (no upward walk) | `project/detect.js` | `project::detect::detect_project_in` | Ported | code review + all detect tests |
| Next.js / Vite / React / vanilla detection incl. config files | `detect.js` | `detect_project_in` | Ported, Tested | `project::detect::tests::detects_*`, `falls_back_to_vanilla` |
| Android detection (manifest, `gradlew`, Gradle DSL, `include ':app'`) and Kotlin vs Java | `detect.js` | `is_android_project`, `android_uses_kotlin` | Ported, Tested | `project::detect::tests::detects_android_kotlin_and_java` |
| Package manager detection (pnpm/yarn/bun/npm) | `detectPackageManager` | `detect_package_manager` | Ported, Tested | `project::detect::tests::detects_next_project_from_dependency` |
| `package.json` validation messages | `readJson` | `read_package_json` | Ported, Tested | `project::detect::tests::invalid_package_json_is_reported`, `dependency_sections_must_be_objects` |
| `setup` fast path: detect → auth → options → app → client → SDK → config | `commands/setup.js` | `commands::setup::run` | Ported, Tested | `setup_flow::setup_creates_resources_and_writes_project_config` |
| Non-interactive setup requires a stored session | `setup.js` | `commands::setup::run` | Ported, Tested | `setup_flow::setup_requires_a_session_in_non_interactive_environments` |
| `--name`, `--redirect`, `--scope` ×N, defaults (`http://localhost:3000/callback`, `AIDC_SCOPES`) | `setup.js` | `commands::setup` | Ported, Tested | `setup_flow::setup_honours_name_redirect_and_scope_flags` |
| Interactive wizard (`--interactive`/`--wizard`, 7-step progress, prompts) | `setup.js`, `ui/progress.js` | `commands::setup` + `StepProgress` | Ported | **Gap:** requires a TTY; only the non-TTY guard is automated (`setup_flow::interactive_setup_requires_a_terminal`) |
| Existing `.aid.json` inspection: drift, SDK range/version, repair hints | `inspectExistingSetup` | `inspect_existing_setup` | Ported, Tested | `setup_flow::setup_inspects_an_existing_config_without_creating_resources`, `setup_flow::setup_reports_drift_and_suggests_repairs` |
| Caret/tilde/exact semver range checks | `sdkRangeCompatibility` | `sdk_range_compatibility` | Ported, Tested | `commands::setup::tests::caret_ranges_match_v1_semantics`, `tilde_and_exact_ranges_match_v1_semantics` |
| Recovery journal `.aid-setup-recovery.json` (stages, 0600, atomic) | `setup.js` | `project_config::write` | Ported, Tested | `setup_flow::setup_refuses_to_run_while_a_recovery_journal_exists` |
| `setup --recover` (deletes leftover resources, `--force` off-TTY, keeps live resources) | `recoverSetup` | `recover_setup` | Ported, Tested | `setup_flow::setup_recover_*` (4 tests) |
| Rollback when client creation or config write fails | `cleanupRemoteResources` | `cleanup_remote_resources`, `finish_failed_setup` | Ported, Tested | `setup_flow::setup_cleans_up_the_app_when_client_creation_fails` |
| SDK install for build-mode projects via the detected package manager | `installSdk` | `sdk::install::install_js_sdk` | Ported, Tested (skip path) | `setup_flow::setup_skips_an_sdk_that_is_already_declared`, `sdk::install::tests::package_install_commands_match_v1` |
| Actual `npm/pnpm/yarn/bun add` execution | `installSdk` | `install_js_sdk` | Ported, Tested (live) | `audit/live_sdk.py`: real `npm install ace-id-sdk` → 0.2.3 installed for v1 and v2 |
| Android SDK download from pinned GitHub release, digest + SHA-256 verification, `libs/` write | `installAndroidSdk` | `sdk::install::install_android_sdk` | Ported, Tested (live bytes) | `sdk::verify::tests::*` + `audit/live_sdk.py` (real 108,945-byte artifact, pinned hash, 302 hop, `sdk_path` written) |
| Pinned Android artifact digest `8fde79fd…` shared with v1 | `setup.js` | `sdk::verify::ANDROID_SDK_SHA256` | Ported, Tested | `sdk::verify::tests::pinned_digest_is_a_sha256_hex_string` |
| Android artifact drift check in `setup` inspection | `inspectExistingSetup` | `inspect_existing_setup` | Ported | code review (needs a fixture artifact) |
| `link <app_id> [--client] [--force]` incl. `already exists` guard and foreign-client rejection | `commands/link.js` | `commands::link` | Ported, Tested | `api_commands::link_writes_project_configuration`, `link_requires_force_to_replace_and_rejects_foreign_clients` |
| App-switch drops the previous app's client/redirect, same-app relink preserves them | `link.js` | `commands::link::run` | Ported | code review; behaviour asserted indirectly by the two link tests |
| `sync` plan output, `--dry-run`, conflict re-check, `synced_at`, unrelated keys preserved | `commands/sync.js` | `commands::sync` | Ported, Tested | `api_commands::sync_*` (4 tests) |
| `sync <app_id>` replaces the link | `sync.js` | `commands::sync::run` | Ported | covered by `link` tests |
| `sync` refuses other issuers / missing or invalid `app_id` / invalid `client_id` | `sync.js` | `commands::sync::run` | Ported, Tested | `api_commands::sync_refuses_other_issuers_and_missing_links` |
| `.aid.json` written atomically with mode 0600 and trailing newline | `writeConfig` | `config::project_config::write` | Ported, Tested | `config::project_config::tests::writes_and_reads_a_project_document`, `security::written_project_config_is_private` |
| v1 configuration file format preserved (same keys, same JSON shape) | `setup.js`, `link.js` | `project_config` | Ported, Tested | `setup_flow::setup_creates_resources_and_writes_project_config`, `api_commands::link_writes_project_configuration` |

## Environment and compatibility

| Feature | v1 source | v2 implementation | Status | Test |
| --- | --- | --- | --- | --- |
| `AIDC_ISSUER` (HTTPS unless loopback, no credentials/query/fragment) | `auth/config.js` | `config::normalize_issuer` | Ported, Tested | `config::tests::issuer_*`, `security::issuer_must_be_https_unless_loopback` |
| `AIDC_CLIENT_ID` (default `aidc-cli`) | `config.js` | `Config::from_env` | Ported, Tested | `config::tests::defaults_match_v1` |
| `AIDC_PORT` (default 7357, 1–65535) | `config.js` | `parse_port` | Ported, Tested | `config::tests::port_must_be_in_range` |
| `AIDC_SCOPES` (default `openid profile email offline_access`) | `config.js` | `normalize_scopes` | Ported, Tested | `config::tests::defaults_match_v1` |
| `AIDC_CALLBACK_TIMEOUT_MS`, `AIDC_REFRESH_LEEWAY_MS`, `AIDC_REQUEST_TIMEOUT_MS` | `config.js` | `parse_positive_int` | Ported, Tested | `config::tests::timeouts_must_be_positive_integers` |
| `BROWSER`, `NO_COLOR`, `TERM=dumb`, `XDG_CONFIG_HOME`, `APPDATA` | `login.js`, `colors.js`, `tokens.js` | `oauth::open_browser`, `colors_enabled`, `config::paths` | Ported, Tested (partially) | `config::paths::tests::xdg_config_home_is_respected` |
| Node-free execution | — | static-ish Rust binary, no runtime | Ported, Tested | `cli_surface` runs the binary directly |
| Existing v1 tokens/`.aid.json` readable (no re-authentication, no data rewrite) | file formats | same paths, same JSON | Ported, Tested | `auth_flow` seeds a v1-shaped `tokens.json`; `api_commands` seeds a v1-shaped `.aid.json` |

## Known gaps (summary)

1. Interactive wizard and spinner animation are TTY-only and therefore not
   automated.
2. Windows/macOS: CI builds **and executes** each binary (`--version`, `--help`,
   `logout`), but OAuth, permissions and browser launch are not exercised there.
   Android/Termux is not verified at all.
3. `sync` and `clients list` issue their two reads sequentially rather than
   concurrently (byte-identical output, one extra round trip on the happy path,
   one request fewer on failure).
4. TLS trust is the bundled root store: a TLS-inspecting proxy cannot be verified
   (documented in `docs/SECURITY.md`).
