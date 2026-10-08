# Changelog

All notable changes to the Ace ID CLI are documented here.

## [0.1.3]

- Complete `--help` command/flag reference, including setup, apps, clients, aliases, and short flags.

### Added
- Version-agnostic JavaScript SDK detection.
- Explicit interactive setup mode with `aid setup --interactive`.
- Authentication request timeouts.
- Concurrent token-refresh deduplication.
- CI validation for syntax, package generation, and CLI smoke tests.

### Changed
- `aid setup` remains the fast default while the wizard is opt-in.
- README now documents aliases, setup modes, authentication, portability, SDK compatibility, configuration, and security.
- API authentication now invalidates locally stored sessions when the server rejects them.
- Token refresh errors are normalized and bounded.
- JavaScript SDK setup no longer pins the CLI to a specific SDK release.

### Security
- Validated issuer configuration and HTTPS enforcement for non-local issuers.
- Bounded OAuth token exchange and API requests.
- Invalid refresh sessions are cleared automatically.
- Malformed token responses are rejected safely.

## [0.1.2]

### Added
- `aid`, `ace-id`, and `aidc` command aliases.
- Fast project setup with sensible defaults.
- Interactive setup wizard support.
- PKCE-based OIDC login.
- Application and OAuth client management.
- Project detection for common JavaScript and Android projects.

### Security
- State validation for the local OAuth callback.
- Loopback-only callback binding.
- Origin validation for authenticated API requests.
- Atomic token and project configuration writes.
- Hash verification for the automatic Android SDK artifact.

[0.1.3]: https://github.com/aceyash-dev/aid-cli/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/aceyash-dev/aid-cli/releases/tag/v0.1.2
