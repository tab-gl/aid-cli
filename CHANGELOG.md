# Changelog

All notable changes to the Ace ID CLI are documented here.

## [Unreleased]

### Security and reliability
- Made help, tables, field summaries, and command descriptions responsive to narrow terminal widths without truncating long resource IDs; improved spinner lifecycle and stream-specific color handling.
- Reduced project detection startup work with parallel filesystem checks and lower-memory Android language detection; report malformed or unreadable project manifests instead of silently misclassifying the project.
- Added step-based progress to interactive setup, including active-step percentage, skipped/failure states, and validation of remote app/client creation responses.
- Made `aid sync` validate issuer and remote app/client identity, refresh linked metadata, remove explicitly cleared stale fields, preserve unrelated settings, and detect local edits during remote reads.
- Prevented relinking from carrying a stale OAuth client or redirect configuration across different applications; verify the selected OAuth client belongs to the linked app before writing `.aid.json`.
- Use collision-resistant temporary filenames for setup and recovery writes so concurrent runs cannot overwrite one another's temporary files.
- Added `aid test-auth` to verify the stored session against the authenticated identity endpoint without exposing credentials.
- Serialized token writes and refreshes across CLI processes to protect rotating refresh tokens.
- Avoided clearing a newer session when a request made with an older access token receives HTTP 401.
- Re-hardened stored session permissions on load and rejected symlink-based token files.
- Added a local setup recovery journal and `aid setup --recover` for interrupted remote resource cleanup.
- Bounded and redacted API, OAuth, and deletion error details.
- Added regression coverage for refresh races, setup recovery, API error redaction, linked-client ownership, and malformed project manifests.

### Fixed
- Keep API, OAuth token, and Android SDK request deadlines active while response bodies are consumed.
- Added a regression test for API responses that send headers but never finish the body.

## [0.1.4]

### Fixed
- Corrected `aid --version` so it prints the actual package version.
- Fixed literal newline escapes in CLI help output.
- Aligned package metadata and lockfile with the documented 0.1.4 release.
- Prevented unrelated or forged OAuth callback requests from terminating a valid login attempt.
- Validated issuer, callback port, timeout, and refresh-leeway configuration at startup.
- Preserved issuer path prefixes for API requests and blocked credential forwarding across redirects.
- Added automated regression-test execution to CI.
- Ignored local `.aid.json` configuration files.

### Security
- OAuth callback requests now validate HTTP method and path and use no-store response headers.
- Invalid security-sensitive environment configuration fails clearly instead of silently falling back.

## [0.1.3]

### Audit fixes
- Added the neutral near-white terminal text hierarchy and corrected muted label styling.
- Fixed screen clearing so `aid` preserves terminal scrollback.
- Added timeouts to automatic Android SDK release and artifact downloads.

### Added
- Version-agnostic JavaScript SDK detection.
- Explicit interactive setup mode with `aid setup --interactive`.
- Authentication request timeouts and concurrent token-refresh deduplication.
- CI validation for syntax, package generation, and CLI smoke tests.

### Security
- Validated issuer configuration and HTTPS enforcement for non-local issuers.
- Invalid refresh sessions are cleared automatically.
- Malformed token responses are rejected safely.

## [0.1.2]

### Added
- `aid`, `ace-id`, and `aidc` command aliases.
- Fast and interactive setup modes.
- PKCE-based OIDC login and app/client management.
- Project detection for common JavaScript and Android projects.

### Security
- State validation for the local OAuth callback.
- Loopback-only callback binding.
- Origin validation for authenticated API requests.
- Atomic token and project configuration writes.
- Hash verification for the automatic Android SDK artifact.

[0.1.4]: https://github.com/aceyash-dev/aid-cli/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/aceyash-dev/aid-cli/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/aceyash-dev/aid-cli/releases/tag/v0.1.2
