//! Resilience and concurrency tests: redirect policy, stale locks, crashes,
//! read-only storage and logout racing a refresh.

mod support;

use std::fs;
use std::time::{Duration, SystemTime};

use support::{command, stderr_of, stdout_of, Env, MockServer, Route};

fn me_route() -> Route {
    Route::json("GET", "/me", 200, r#"{"name":"Yash Gupta","sub":"user-1"}"#)
}

fn refresh_route(delay_ms: u64) -> Route {
    Route::json(
        "POST",
        "/token",
        200,
        r#"{"access_token":"at-2","token_type":"Bearer","expires_in":3600}"#,
    )
    .delay(Duration::from_millis(delay_ms))
}

fn lock_path(env: &Env) -> std::path::PathBuf {
    env.config_home.join("aidc").join("tokens.json.lock")
}

fn write_lock(env: &Env, contents: &str) {
    let dir = env.config_home.join("aidc");
    fs::create_dir_all(&dir).unwrap();
    fs::write(lock_path(env), contents).unwrap();
}

/* ── Redirect policy ─────────────────────────────────────────────────────── */

#[test]
fn redirect_chains_are_followed_within_the_budget() {
    use ace_id_cli::http::Request;

    let server = MockServer::start(vec![
        Route::json("GET", "/r1", 302, "").header("location", "/r2"),
        Route::json("GET", "/r2", 302, "").header("location", "/target"),
        Route::json("GET", "/target", 200, r#"{"ok":true}"#),
    ]);

    let response = Request::new(
        "GET",
        &format!("{}/r1", server.base),
        Duration::from_secs(5),
    )
    .follow_redirects(5)
    .send()
    .expect("two hops are within budget");
    assert_eq!(response.status, 200);
    assert_eq!(server.requests().len(), 3);
}

#[test]
fn redirect_budget_is_enforced() {
    use ace_id_cli::http::{HttpError, Request};

    let server = MockServer::start(vec![
        Route::json("GET", "/r1", 302, "").header("location", "/r2"),
        Route::json("GET", "/r2", 302, "").header("location", "/r3"),
        Route::json("GET", "/r3", 302, "").header("location", "/r4"),
        Route::json("GET", "/r4", 200, r#"{"ok":true}"#),
    ]);

    let result = Request::new(
        "GET",
        &format!("{}/r1", server.base),
        Duration::from_secs(5),
    )
    .follow_redirects(2)
    .send();
    match result {
        Err(HttpError::Redirect(reason)) => {
            assert!(reason.contains("more than 2 redirects"), "{reason}")
        }
        other => panic!("expected a redirect error, got {other:?}"),
    }
    // The budget stopped the chain before the final hop.
    assert!(server.requests_to("GET", "/r4").is_empty());
}

#[test]
fn credentials_are_not_forwarded_across_a_redirect() {
    use ace_id_cli::http::Request;

    let server = MockServer::start(vec![
        Route::json("GET", "/moved", 302, "").header("location", "/target"),
        Route::json("GET", "/target", 200, r#"{"ok":true}"#),
    ]);

    Request::new(
        "GET",
        &format!("{}/moved", server.base),
        Duration::from_secs(5),
    )
    .header("authorization", "Bearer should-not-leak")
    .follow_redirects(3)
    .send()
    .expect("redirect should be followed");

    let first = &server.requests_to("GET", "/moved")[0];
    assert_eq!(
        first.header("authorization"),
        Some("Bearer should-not-leak")
    );
    let second = &server.requests_to("GET", "/target")[0];
    assert_eq!(
        second.header("authorization"),
        None,
        "the redirected request must not carry the credential"
    );
}

/* ── Lock recovery and crashes ───────────────────────────────────────────── */

#[test]
fn a_lock_left_by_a_dead_process_is_recovered() {
    // `logout` always takes the session lock, so it exercises stale-lock recovery.
    let env = Env::new("lock-dead-pid");
    env.seed_tokens("at-1", 3_600_000, None);
    // A pid that cannot be running: 2^31-1 is above the default pid_max.
    write_lock(&env, r#"{"pid":2147483647,"created_at":0}"#);

    let output = env.run(&["logout"], &[]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(!lock_path(&env).exists(), "the stale lock must be removed");
    assert!(!env.tokens_path().exists());
}

#[test]
fn an_ancient_lock_without_a_pid_is_recovered() {
    let env = Env::new("lock-no-pid");
    env.seed_tokens("at-1", 3_600_000, None);
    write_lock(&env, "not json at all");

    let handle = fs::File::options()
        .write(true)
        .open(lock_path(&env))
        .unwrap();
    let old = SystemTime::now() - Duration::from_secs(120);
    handle
        .set_times(fs::FileTimes::new().set_modified(old))
        .unwrap();
    drop(handle);

    let output = env.run(&["logout"], &[]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(!lock_path(&env).exists());
}

#[test]
fn logout_during_a_refresh_leaves_no_session() {
    // A refresh is in flight (the token endpoint is slow) while `logout` runs.
    // Whatever the interleaving, the session must end up deleted.
    let server = MockServer::start(vec![me_route(), refresh_route(400)]);
    let env = Env::new("logout-race");
    env.seed_tokens("at-1", -1_000, Some("rt-1"));

    let refreshing = command(&env.dir, &env.config_home, &[("AIDC_ISSUER", &server.base)])
        .arg("whoami")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn whoami");

    std::thread::sleep(Duration::from_millis(120));

    let logout = env.run(&["logout"], &[("AIDC_ISSUER", &server.base)]);
    let refresh = refreshing.wait_with_output().expect("whoami output");

    assert!(
        logout.status.success(),
        "logout must succeed: {}",
        stderr_of(&logout)
    );
    assert!(
        !env.tokens_path().exists(),
        "no session may survive a logout (whoami exit={:?}, stderr={})",
        refresh.status.code(),
        stderr_of(&refresh)
    );
    assert!(!lock_path(&env).exists(), "no lock file may be left behind");
}

/* ── Storage failures ────────────────────────────────────────────────────── */

#[cfg(unix)]
#[test]
fn a_widened_config_directory_is_repaired() {
    use std::os::unix::fs::PermissionsExt;

    // v1 chmods the token directory to 0700 on every access; v2 does the same, so
    // a directory someone widened is tightened again rather than used as-is.
    let server = MockServer::start(vec![me_route()]);
    let env = Env::new("dir-mode");
    env.seed_tokens("at-1", 3_600_000, None);

    let dir = env.config_home.join("aidc");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o777)).unwrap();

    let output = env.run(&["whoami"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "the directory must be tightened");
}

#[test]
fn unwritable_storage_is_reported_without_panicking() {
    // A true ENOSPC cannot be produced without a tiny filesystem, so the storage
    // path is made unusable instead: `aidc` exists as a regular file, so neither
    // the lock nor the token file can be created.
    let env = Env::new("unwritable");
    let blocking = env.config_home.join("aidc");
    fs::write(&blocking, "not a directory").unwrap();

    let output = env.run(&["logout"], &[]);
    assert_eq!(output.status.code(), Some(1), "{:?}", stdout_of(&output));
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("Could not create the configuration directory")
            || stderr.contains("Could not write")
            || stderr.contains("Not a directory"),
        "expected a clean storage error, got: {stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "the CLI must not panic on storage failures"
    );

    // The blocking file is untouched: nothing was written through it.
    assert_eq!(fs::read_to_string(&blocking).unwrap(), "not a directory");
}

/* ── Proxy behaviour ─────────────────────────────────────────────────────── */

/// Serialises the two tests that mutate process-wide proxy variables.
static PROXY_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A recorded proxy request: request line plus lower-cased headers.
type ProxyRequest = (String, Vec<(String, String)>);
type ProxyLog = std::sync::Arc<std::sync::Mutex<Vec<ProxyRequest>>>;

fn proxy_server(require_auth: bool) -> (String, ProxyLog) {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind proxy");
    let port = listener.local_addr().unwrap().port();
    let seen: ProxyLog = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_thread = std::sync::Arc::clone(&seen);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buffer = Vec::new();
            let mut chunk = [0u8; 1024];
            while let Ok(read) = stream.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                buffer.extend_from_slice(&chunk[..read]);
                if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let head = String::from_utf8_lossy(&buffer).to_string();
            let mut lines = head.lines();
            let request_line = lines.next().unwrap_or("").to_string();
            let headers: Vec<(String, String)> = lines
                .filter_map(|line| line.split_once(':'))
                .map(|(name, value)| (name.trim().to_lowercase(), value.trim().to_string()))
                .collect();
            seen_thread
                .lock()
                .unwrap()
                .push((request_line.clone(), headers.clone()));

            let authorized = headers.iter().any(|(name, value)| {
                name == "proxy-authorization" && value == "Basic dXNlcjpwYXNz"
            });
            let body = r#"{"proxied":true}"#;
            let (status, payload) = if require_auth && !authorized {
                (
                    "407 Proxy Authentication Required",
                    r#"{"message":"auth required"}"#,
                )
            } else {
                ("200 OK", body)
            };
            let challenge = if status.starts_with("407") {
                "proxy-authenticate: Basic realm=\"ace-id-test\"\r\n"
            } else {
                ""
            };
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\n{challenge}content-length: {}\r\nconnection: close\r\n\r\n{payload}",
                payload.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (format!("http://127.0.0.1:{port}"), seen)
}

#[test]
fn authenticated_proxies_are_reported_not_silently_ignored() {
    // ureq 2.x does not perform proxy authentication (no 407 retry, no CONNECT
    // credentials), so a proxy that demands auth must fail loudly instead of
    // appearing to work. Credentials in the proxy URL are never forwarded to the
    // origin, and are not leaked into the error text either.
    use ace_id_cli::http::Request;

    let _guard = PROXY_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (proxy, seen) = proxy_server(true);
    std::env::set_var(
        "HTTP_PROXY",
        format!("http://user:pass@{}", proxy.trim_start_matches("http://")),
    );
    std::env::remove_var("HTTPS_PROXY");
    std::env::remove_var("NO_PROXY");
    std::env::remove_var("no_proxy");

    let result = Request::new("GET", "http://192.0.2.1/me", Duration::from_secs(5)).send();

    std::env::remove_var("HTTP_PROXY");
    let requests = seen.lock().unwrap().clone();

    let status = match result {
        Ok(response) => response.status,
        Err(other) => panic!("unexpected transport failure: {other:?}"),
    };
    assert_eq!(status, 407, "an auth-required proxy must surface as 407");
    assert_eq!(requests.len(), 1, "no retry is attempted: {requests:?}");
    assert!(
        !requests[0]
            .1
            .iter()
            .any(|(name, _)| name == "proxy-authorization"),
        "proxy credentials must not be invented: {requests:?}"
    );
}

#[test]
fn no_proxy_excludes_hosts_from_proxy_selection() {
    // Selection is decided before any socket is opened, so this needs no network:
    // an excluded host must resolve to "no proxy", everything else to the proxy.
    let _guard = PROXY_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("HTTP_PROXY", "http://127.0.0.1:3128");
    std::env::remove_var("HTTPS_PROXY");
    std::env::remove_var("no_proxy");

    for (no_proxy, url, expected) in [
        ("blocked.invalid", "http://blocked.invalid/me", false),
        ("blocked.invalid", "http://api.blocked.invalid/me", false),
        (".blocked.invalid", "http://api.blocked.invalid/me", false),
        ("*", "http://anything.invalid/me", false),
        ("other.invalid", "http://blocked.invalid/me", true),
        ("", "http://blocked.invalid/me", true),
        // Loopback is never proxied, whatever NO_PROXY says.
        ("", "http://127.0.0.1:9000/me", false),
    ] {
        std::env::set_var("NO_PROXY", no_proxy);
        let selected = ace_id_cli::http::proxy_from_env(url).is_some();
        assert_eq!(
            selected,
            expected,
            "NO_PROXY={no_proxy:?} url={url} should{} use the proxy",
            if expected { "" } else { " not" }
        );
    }

    std::env::remove_var("HTTP_PROXY");
    std::env::remove_var("NO_PROXY");
}
