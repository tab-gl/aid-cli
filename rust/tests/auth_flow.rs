//! End-to-end authentication tests: login over a real loopback callback, token
//! persistence, refresh, concurrency, 401 handling and logout.

mod support;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::Stdio;

use support::{command, free_port, stderr_of, stdout_of, Env, MockServer, Route};

const TOKEN_OK: &str = r#"{"access_token":"at-1","refresh_token":"rt-1","id_token":"id-1",
    "token_type":"Bearer","scope":"openid profile","expires_in":3600}"#;

fn me_route(account: &str) -> Route {
    Route::json(
        "GET",
        "/me",
        200,
        &format!(r#"{{"name":"{account}","email":"user@example.com","sub":"sub-1"}}"#),
    )
}

/// Drive `aid login` end to end: read the authorize URL, then play the browser.
fn login(env: &Env, server: &MockServer, wrong_state_first: bool) -> std::process::Output {
    let port = free_port();
    let mut child = command(
        &env.dir,
        &env.config_home,
        &[
            ("AIDC_ISSUER", &server.base),
            ("AIDC_PORT", &port.to_string()),
        ],
    )
    .arg("login")
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("spawn login");

    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);
    let mut authorize_url = String::new();
    let mut line = String::new();
    while reader.read_line(&mut line).expect("read line") > 0 {
        if line.contains("/auth?") {
            authorize_url = line.trim().to_string();
            break;
        }
        line.clear();
    }
    assert!(
        authorize_url.contains("code_challenge_method=S256"),
        "authorize URL missing PKCE parameters: {authorize_url}"
    );
    let state = query_param(&authorize_url, "state").expect("state parameter");
    assert_eq!(
        query_param(&authorize_url, "response_type").as_deref(),
        Some("code")
    );

    if wrong_state_first {
        let (status, body) = http_get(port, "/callback?state=forged&code=stolen");
        assert_eq!(status, 400, "forged callbacks must be rejected");
        assert!(body.contains("Invalid state"));
    }

    let (status, body) = http_get(port, &format!("/callback?state={state}&code=code-123"));
    assert_eq!(status, 200);
    assert!(body.contains("Signed in"));

    // Drain the rest of the output and wait for exit.
    let mut rest = String::new();
    let _ = reader.read_to_string(&mut rest);
    let mut stderr = String::new();
    if let Some(mut handle) = child.stderr.take() {
        let _ = handle.read_to_string(&mut stderr);
    }
    let status = child.wait().expect("wait");
    std::process::Output {
        status,
        stdout: rest.into_bytes(),
        stderr: stderr.into_bytes(),
    }
}

fn query_param(url: &str, name: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{name}=")).map(str::to_string))
}

fn http_get(port: u16, target: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect callback");
    let request = format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("write request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    let status = response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    (status, response)
}

#[test]
fn login_completes_the_pkce_flow_and_stores_tokens() {
    let server = MockServer::start(vec![Route::json("POST", "/token", 200, TOKEN_OK)]);
    let env = Env::new("login");

    let output = login(&env, &server, false);
    assert!(
        output.status.success(),
        "login failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout_of(&output).contains("Signed in. Tokens stored."));

    let request = &server.requests_to("POST", "/token")[0];
    assert_eq!(
        request.form_value("grant_type").as_deref(),
        Some("authorization_code")
    );
    assert_eq!(request.form_value("code").as_deref(), Some("code-123"));
    assert_eq!(request.form_value("client_id").as_deref(), Some("aidc-cli"));
    let verifier = request.form_value("code_verifier").expect("verifier");
    assert!(verifier.len() >= 43, "PKCE verifier must be long enough");

    let tokens: serde_json::Value =
        serde_json::from_str(env.read(".config/aidc/tokens.json").trim()).expect("tokens json");
    assert_eq!(tokens["access_token"], "at-1");
    assert_eq!(tokens["refresh_token"], "rt-1");
    assert!(tokens["expires_at"].as_i64().unwrap() > 0);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(env.tokens_path())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "token file must be private");
    }
}

#[test]
fn forged_callbacks_do_not_consume_the_login() {
    let server = MockServer::start(vec![Route::json("POST", "/token", 200, TOKEN_OK)]);
    let env = Env::new("login-state");

    let output = login(&env, &server, true);
    assert!(
        output.status.success(),
        "login failed after a forged callback: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(server.requests_to("POST", "/token").len(), 1);
}

#[test]
fn login_reports_token_endpoint_failures() {
    let server = MockServer::start(vec![Route::json(
        "POST",
        "/token",
        400,
        r#"{"error":"invalid_grant","error_description":"code expired"}"#,
    )]);
    let env = Env::new("login-fail");

    let output = login(&env, &server, false);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Token exchange failed (400)"), "{stderr}");
    assert!(stderr.contains("code expired"), "{stderr}");
    assert!(!env.tokens_path().exists(), "no tokens on failure");
}

#[test]
fn login_rejects_a_token_response_without_an_access_token() {
    let server = MockServer::start(vec![Route::json("POST", "/token", 200, r#"{"ok":true}"#)]);
    let env = Env::new("login-empty");

    let output = login(&env, &server, false);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no access token"));
}

#[test]
fn expired_tokens_are_refreshed_transparently() {
    let server = MockServer::start(vec![
        me_route("Yash Gupta"),
        Route::json(
            "POST",
            "/token",
            200,
            r#"{"access_token":"at-2","token_type":"Bearer","expires_in":3600}"#,
        ),
    ]);
    let env = Env::new("refresh");
    env.seed_tokens("at-1", -1_000, Some("rt-1"));

    let output = env.run(&["whoami"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(stdout_of(&output).contains("Yash Gupta"));

    let refresh = &server.requests_to("POST", "/token")[0];
    assert_eq!(
        refresh.form_value("grant_type").as_deref(),
        Some("refresh_token")
    );
    assert_eq!(refresh.form_value("refresh_token").as_deref(), Some("rt-1"));

    // The API call used the refreshed token, and the store was updated.
    assert_eq!(
        server.requests_to("GET", "/me")[0].header("authorization"),
        Some("Bearer at-2")
    );
    let tokens: serde_json::Value =
        serde_json::from_str(env.read(".config/aidc/tokens.json").trim()).expect("tokens");
    assert_eq!(tokens["access_token"], "at-2");
    assert_eq!(
        tokens["refresh_token"], "rt-1",
        "refresh token is preserved"
    );
}

#[test]
fn invalid_refresh_sessions_are_cleared() {
    let server = MockServer::start(vec![Route::json(
        "POST",
        "/token",
        400,
        r#"{"error":"invalid_grant"}"#,
    )]);
    let env = Env::new("refresh-invalid");
    env.seed_tokens("at-1", -1_000, Some("rt-1"));

    let output = env.run(&["whoami"], &[("AIDC_ISSUER", &server.base)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("session is no longer valid"));
    assert!(!env.tokens_path().exists(), "dead session must be removed");
}

#[test]
fn expired_session_without_refresh_token_is_cleared() {
    let env = Env::new("no-refresh");
    env.seed_tokens("at-1", -1_000, None);

    let output = env.run(&["whoami"], &[("AIDC_ISSUER", "http://127.0.0.1:9")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("no refresh token is stored"));
    assert!(!env.tokens_path().exists());
}

#[test]
fn rejected_access_tokens_clear_the_session() {
    let server = MockServer::start(vec![Route::json(
        "GET",
        "/me",
        401,
        r#"{"message":"expired"}"#,
    )]);
    let env = Env::new("unauthorized");
    env.seed_tokens("at-1", 3_600_000, None);

    let output = env.run(&["whoami"], &[("AIDC_ISSUER", &server.base)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("Server rejected the access token"));
    assert!(
        !env.tokens_path().exists(),
        "401 must drop the stored session"
    );
}

#[test]
fn concurrent_processes_share_a_single_refresh() {
    let server = MockServer::start(vec![
        me_route("Yash Gupta"),
        Route::json(
            "POST",
            "/token",
            200,
            r#"{"access_token":"at-2","token_type":"Bearer","expires_in":3600}"#,
        )
        .delay(std::time::Duration::from_millis(400)),
    ]);
    let env = Env::new("refresh-race");
    env.seed_tokens("at-1", -1_000, Some("rt-1"));

    let spawn = || {
        command(&env.dir, &env.config_home, &[("AIDC_ISSUER", &server.base)])
            .arg("whoami")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn whoami")
    };

    let first = spawn();
    let second = spawn();
    let first = first.wait_with_output().expect("first output");
    let second = second.wait_with_output().expect("second output");

    assert!(first.status.success(), "{}", stderr_of(&first));
    assert!(second.status.success(), "{}", stderr_of(&second));
    assert_eq!(
        server.requests_to("POST", "/token").len(),
        1,
        "only one process may refresh; the other must reuse the new token"
    );
}

#[test]
fn logout_deletes_the_stored_session() {
    let env = Env::new("logout");
    env.seed_tokens("at-1", 3_600_000, Some("rt-1"));
    assert!(env.tokens_path().exists());

    let output = env.run(&["logout"], &[]);
    assert!(output.status.success());
    assert!(stdout_of(&output).contains("Signed out. Local tokens deleted."));
    assert!(!env.tokens_path().exists());
}
