//! Security-focused integration tests: configuration validation, unsafe token
//! files, secret redaction, permission hardening and SDK integrity checks.

mod support;

use std::time::Duration;

use support::{stderr_of, stdout_of, Env, MockServer, Route};

#[test]
fn issuer_must_be_https_unless_loopback() {
    let env = Env::new("issuer");
    env.seed_tokens("at-1", 3_600_000, None);

    let output = env.run(&["apps", "list"], &[("AIDC_ISSUER", "http://example.com")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("AIDC_ISSUER must use HTTPS unless it targets localhost."));

    let output = env.run(&["apps", "list"], &[("AIDC_ISSUER", "not a url")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("AIDC_ISSUER must be a valid absolute URL."));

    let output = env.run(
        &["apps", "list"],
        &[("AIDC_ISSUER", "https://user:pw@example.com")],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("must not include credentials"));
}

#[test]
fn invalid_timeout_and_port_values_are_rejected() {
    let env = Env::new("env-values");
    env.seed_tokens("at-1", 3_600_000, None);

    let output = env.run(
        &["apps", "list"],
        &[
            ("AIDC_ISSUER", "http://127.0.0.1:9"),
            ("AIDC_REQUEST_TIMEOUT_MS", "0"),
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("must be positive integers"));

    let output = env.run(
        &["apps", "list"],
        &[
            ("AIDC_ISSUER", "http://127.0.0.1:9"),
            ("AIDC_PORT", "70000"),
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("AIDC_PORT must be an integer between 1 and 65535."));
}

#[test]
fn version_and_help_do_not_require_valid_configuration() {
    // v1 evaluated its config module at import time, so a bad AIDC_ISSUER broke
    // even `--version`. v2 resolves configuration lazily for commands that need it.
    let env = Env::new("lazy-config");
    let output = env.run(&["--version"], &[("AIDC_ISSUER", "not a url")]);
    assert!(output.status.success());
    assert!(stdout_of(&output).starts_with("ace-id "));

    let output = env.run(&["--help"], &[("AIDC_ISSUER", "not a url")]);
    assert!(output.status.success());
}

#[cfg(unix)]
#[test]
fn symlinked_token_files_are_refused() {
    let env = Env::new("symlink");
    let real = env.dir.join("elsewhere.json");
    std::fs::write(&real, r#"{"access_token":"at-1"}"#).unwrap();
    let dir = env.config_home.join("aidc");
    std::fs::create_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(&real, dir.join("tokens.json")).unwrap();

    let output = env.run(&["whoami"], &[("AIDC_ISSUER", "http://127.0.0.1:9")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("invalid or unsafe"));
}

#[test]
fn corrupt_token_files_are_refused() {
    let env = Env::new("corrupt");
    let dir = env.config_home.join("aidc");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("tokens.json"), "{ not json").unwrap();

    let output = env.run(&["whoami"], &[("AIDC_ISSUER", "http://127.0.0.1:9")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("invalid or unsafe"));
}

#[test]
fn token_documents_with_wrong_types_are_refused() {
    let env = Env::new("types");
    let dir = env.config_home.join("aidc");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("tokens.json"),
        r#"{"access_token":"at-1","expires_at":"tomorrow"}"#,
    )
    .unwrap();

    let output = env.run(&["whoami"], &[("AIDC_ISSUER", "http://127.0.0.1:9")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("invalid or unsafe"));
}

#[cfg(unix)]
#[test]
fn written_project_config_is_private() {
    use std::os::unix::fs::PermissionsExt;

    let server = MockServer::start(vec![Route::json(
        "GET",
        "/apps/a1",
        200,
        r#"{"id":"a1","name":"Alpha"}"#,
    )]);
    let env = Env::new("permissions");
    env.seed_tokens("at-1", 3_600_000, None);

    let output = env.run(&["link", "a1"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));

    let mode = std::fs::metadata(env.dir.join(".aid.json"))
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, ".aid.json must be private");

    let token_mode = std::fs::metadata(env.tokens_path())
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(token_mode, 0o600);
}

#[test]
fn bearer_tokens_never_appear_in_error_output() {
    let server = MockServer::start(vec![Route::json(
        "GET",
        "/apps",
        403,
        r#"{"message":"denied for Bearer at-1 and https://identity.example/token"}"#,
    )]);
    let env = Env::new("redaction");
    env.seed_tokens("at-1", 3_600_000, None);

    let output = env.run(&["apps", "list"], &[("AIDC_ISSUER", &server.base)]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = stderr_of(&output);
    assert!(stderr.contains("Forbidden"), "{stderr}");
    assert!(!stderr.contains("at-1"), "access token leaked: {stderr}");
    assert!(!stderr.contains("identity.example"), "url leaked: {stderr}");
}

#[test]
fn html_error_pages_do_not_leak_into_output() {
    let server = MockServer::start(vec![Route::json(
        "GET",
        "/apps",
        500,
        "<!DOCTYPE html><html><body>Internal error</body></html>",
    )]);
    let env = Env::new("html-error");
    env.seed_tokens("at-1", 3_600_000, None);

    let output = env.run(&["apps", "list"], &[("AIDC_ISSUER", &server.base)]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = stderr_of(&output);
    assert!(stderr.contains("HTTP error"), "{stderr}");
    assert!(!stderr.contains("<html"), "{stderr}");
}

#[test]
fn api_paths_cannot_escape_the_issuer() {
    // Identifiers are URL-encoded, so a hostile id cannot traverse the API.
    let server = MockServer::start(vec![Route::json(
        "GET",
        "/apps/..%2F..%2Fsteal",
        404,
        r#"{"message":"not found"}"#,
    )]);
    let env = Env::new("traversal");
    env.seed_tokens("at-1", 3_600_000, None);

    let output = env.run(
        &["apps", "get", "../../steal"],
        &[("AIDC_ISSUER", &server.base)],
    );
    assert_eq!(output.status.code(), Some(1));
    // The request stayed on the issuer host and was simply not found.
    let requests = server.requests();
    assert!(
        !requests.is_empty(),
        "the request must still reach the issuer"
    );
    assert!(requests[0].target.starts_with("/apps/"));
}

#[test]
fn slow_endpoints_time_out_without_hanging() {
    let server = MockServer::start(vec![
        Route::json("GET", "/me", 200, "{}").delay(Duration::from_secs(3))
    ]);
    let env = Env::new("slow");
    env.seed_tokens("at-1", 3_600_000, None);

    let started = std::time::Instant::now();
    let output = env.run(
        &["whoami"],
        &[
            ("AIDC_ISSUER", &server.base),
            ("AIDC_REQUEST_TIMEOUT_MS", "200"),
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("timed out"));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the CLI must not wait for the full server delay"
    );
}
