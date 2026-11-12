//! Command-level tests against a local mock API server.

mod support;

use std::time::Duration;
use support::{stderr_of, stdout_of, Env, MockServer, Route};

fn apps_route() -> Route {
    Route::json(
        "GET",
        "/apps",
        200,
        r#"[
            {"id":"a1","name":"Alpha","description":"first"},
            {"id":"a2","name":"Beta","description":null}
        ]"#,
    )
}

fn app_route() -> Route {
    Route::json(
        "GET",
        "/apps/a1",
        200,
        r#"{"id":"a1","name":"Alpha","description":"first","created_at":"2026-01-01T00:00:00.000Z"}"#,
    )
}

fn me_route() -> Route {
    Route::json(
        "GET",
        "/me",
        200,
        r#"{"name":"Yash Gupta","email":"yash@example.com","sub":"user-1"}"#,
    )
}

#[test]
fn whoami_prints_the_signed_in_account() {
    let server = MockServer::start(vec![me_route()]);
    let env = Env::new("whoami");
    env.seed_tokens("token-1", 3_600_000, None);

    let output = env.run(&["whoami"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(stdout.contains("Signed in as"));
    assert!(stdout.contains("Yash Gupta"));
    assert!(stdout.contains("yash@example.com"));

    // The bearer token is attached to the request.
    let request = &server.requests_to("GET", "/me")[0];
    assert_eq!(request.header("authorization"), Some("Bearer token-1"));
}

#[test]
fn whoami_json_matches_v1_shape() {
    let server = MockServer::start(vec![me_route()]);
    let env = Env::new("whoami-json");
    env.seed_tokens("token-1", 3_600_000, None);

    let output = env.run(&["whoami", "--json"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let value: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).expect("json");
    assert_eq!(value["name"], "Yash Gupta");
    assert_eq!(value["email"], "yash@example.com");
    assert_eq!(value["sub"], "user-1");
}

#[test]
fn whoami_requires_a_session() {
    let env = Env::new("whoami-anon");
    let output = env.run(&["whoami"], &[("AIDC_ISSUER", "http://127.0.0.1:9")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("Not signed in. Run `aid login` first."));
}

#[test]
fn test_auth_reports_issuer_and_client() {
    let server = MockServer::start(vec![me_route()]);
    let env = Env::new("test-auth");
    env.seed_tokens("token-1", 3_600_000, None);

    let output = env.run(&["test-auth", "--json"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let value: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).expect("json");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["issuer"], server.base);
    assert_eq!(value["client_id"], "aidc-cli");
    assert_eq!(value["account"], "Yash Gupta");
}

#[test]
fn apps_list_renders_a_table_and_supports_filters() {
    let server = MockServer::start(vec![apps_route()]);
    let env = Env::new("apps-list");
    env.seed_tokens("token-1", 3_600_000, None);

    let output = env.run(&["apps", "list"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(stdout.contains("ID"));
    assert!(stdout.contains("Alpha"));
    assert!(stdout.contains("Beta"));

    let output = env.run(
        &["apps", "list", "--search", "alpha"],
        &[("AIDC_ISSUER", &server.base)],
    );
    let stdout = stdout_of(&output);
    assert!(stdout.contains("Alpha"));
    assert!(!stdout.contains("Beta"));

    let output = env.run(
        &["apps", "list", "--limit", "1", "--json"],
        &[("AIDC_ISSUER", &server.base)],
    );
    let value: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).expect("json");
    assert_eq!(value.as_array().map(Vec::len), Some(1));
    assert_eq!(value[0]["id"], "a1");
}

#[test]
fn apps_list_rejects_unexpected_payloads() {
    let server = MockServer::start(vec![Route::json(
        "GET",
        "/apps",
        200,
        r#"{"not":"a list"}"#,
    )]);
    let env = Env::new("apps-bad");
    env.seed_tokens("token-1", 3_600_000, None);
    let output = env.run(&["apps", "list"], &[("AIDC_ISSUER", &server.base)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("unexpected application list"));
}

#[test]
fn apps_create_sends_json_and_reports_the_id() {
    let server = MockServer::start(vec![Route::json(
        "POST",
        "/apps",
        201,
        r#"{"id":"a9","name":"New App"}"#,
    )]);
    let env = Env::new("apps-create");
    env.seed_tokens("token-1", 3_600_000, None);

    let output = env.run(
        &["apps", "create", "-n", "New App", "-d", "desc"],
        &[("AIDC_ISSUER", &server.base)],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(stdout_of(&output).contains("Created New App"));
    assert!(stdout_of(&output).contains("a9"));

    let request = &server.requests_to("POST", "/apps")[0];
    assert_eq!(request.header("content-type"), Some("application/json"));
    let body = request.json();
    assert_eq!(body["name"], "New App");
    assert_eq!(body["description"], "desc");
}

#[test]
fn apps_get_and_update_round_trip() {
    let server = MockServer::start(vec![
        app_route(),
        Route::json("PATCH", "/apps/a1", 200, r#"{"id":"a1","name":"Renamed"}"#),
    ]);
    let env = Env::new("apps-get");
    env.seed_tokens("token-1", 3_600_000, None);

    let output = env.run(
        &["apps", "get", "a1", "--json"],
        &[("AIDC_ISSUER", &server.base)],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));
    let value: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).expect("json");
    assert_eq!(value["id"], "a1");
    assert_eq!(value["description"], "first");

    let output = env.run(
        &["apps", "update", "a1", "--name", "Renamed"],
        &[("AIDC_ISSUER", &server.base)],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(stdout_of(&output).contains("Updated Renamed"));
    assert_eq!(
        server.requests_to("PATCH", "/apps/a1")[0].json()["name"],
        "Renamed"
    );
}

#[test]
fn apps_delete_handles_multiple_ids_and_404() {
    let server = MockServer::start(vec![
        Route::json("DELETE", "/apps/a1", 204, ""),
        Route::json("DELETE", "/apps/a2", 204, ""),
        Route::json("DELETE", "/apps/missing", 404, r#"{"message":"gone"}"#),
    ]);
    let env = Env::new("apps-delete");
    env.seed_tokens("token-1", 3_600_000, None);

    let output = env.run(
        &["apps", "delete", "a1", "a2"],
        &[("AIDC_ISSUER", &server.base)],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert_eq!(stdout_of(&output).matches("Deleted").count(), 2);

    let output = env.run(
        &["apps", "delete", "missing"],
        &[("AIDC_ISSUER", &server.base)],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("No app found with ID missing"));
}

#[test]
fn clients_create_prints_the_secret_once() {
    let server = MockServer::start(vec![Route::json(
        "POST",
        "/clients",
        201,
        r#"{"client_id":"c1","name":"CLI","public":false,"client_secret":"s3cret"}"#,
    )]);
    let env = Env::new("clients-create");
    env.seed_tokens("token-1", 3_600_000, None);

    let output = env.run(
        &[
            "clients",
            "create",
            "-n",
            "CLI",
            "-r",
            "http://localhost:3000/callback",
            "-s",
            "openid",
        ],
        &[("AIDC_ISSUER", &server.base)],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(stdout.contains("Client Secret"));
    assert!(stdout.contains("s3cret"));
    assert!(stderr_of(&output).contains("Save this secret now"));

    let body = server.requests_to("POST", "/clients")[0].json();
    assert_eq!(body["redirect_uris"][0], "http://localhost:3000/callback");
    assert_eq!(body["scopes"][0], "openid");
}

#[test]
fn clients_list_reports_link_diagnostics() {
    let server = MockServer::start(vec![
        Route::json(
            "GET",
            "/clients",
            200,
            r#"[{"client_id":"c1","name":"CLI","public":true,"app_id":"a1","redirect_uris":["http://localhost:3000/callback"]},
                {"client_id":"c2","name":"Orphan","public":true,"app_id":""}]"#,
        ),
        apps_route(),
    ]);
    let env = Env::new("clients-list");
    env.seed_tokens("token-1", 3_600_000, None);
    env.write(
        ".aid.json",
        r#"{"issuer":"__ISSUER__","app_id":"a2","client_id":"c1","redirect_uri":"http://localhost:3000/callback"}"#
            .replace("__ISSUER__", &server.base)
            .as_str(),
    );

    let output = env.run(&["clients", "list"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(stdout.contains("MISLINKED"));
    assert!(stdout.contains("ORPHANED: no app"));

    let output = env.run(
        &["clients", "list", "--json"],
        &[("AIDC_ISSUER", &server.base)],
    );
    let value: serde_json::Value = serde_json::from_str(stdout_of(&output).trim()).expect("json");
    assert_eq!(value[0]["relationship_status"], "MISLINKED");
    assert_eq!(value[0]["application_name"], "Alpha");
    assert_eq!(value[1]["relationship_status"], "ORPHANED: no app");
}

#[test]
fn link_writes_project_configuration() {
    let server = MockServer::start(vec![
        app_route(),
        Route::json(
            "GET",
            "/clients/c1",
            200,
            r#"{"client_id":"c1","app_id":"a1"}"#,
        ),
    ]);
    let env = Env::new("link");
    env.seed_tokens("token-1", 3_600_000, None);
    env.write(
        "package.json",
        r#"{"name":"demo","dependencies":{"next":"14.0.0"}}"#,
    );

    let output = env.run(
        &["link", "a1", "--client", "c1"],
        &[("AIDC_ISSUER", &server.base)],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(stdout_of(&output).contains("Linked Alpha to this folder."));

    let document: serde_json::Value =
        serde_json::from_str(env.read(".aid.json").trim()).expect("json");
    assert_eq!(document["issuer"], server.base);
    assert_eq!(document["app_id"], "a1");
    assert_eq!(document["client_id"], "c1");
    assert_eq!(document["project_type"], "next");
    assert_eq!(document["sdk"], "ace-id-sdk");
}

#[test]
fn link_requires_force_to_replace_and_rejects_foreign_clients() {
    let server = MockServer::start(vec![
        app_route(),
        Route::json(
            "GET",
            "/clients/c2",
            200,
            r#"{"client_id":"c2","app_id":"a2"}"#,
        ),
    ]);
    let env = Env::new("link-guard");
    env.seed_tokens("token-1", 3_600_000, None);
    env.write(
        ".aid.json",
        &format!(r#"{{"issuer":"{}","app_id":"a1"}}"#, server.base),
    );

    let output = env.run(&["link", "a1"], &[("AIDC_ISSUER", &server.base)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("already exists"));

    let output = env.run(
        &["link", "a1", "--client", "c2", "--force"],
        &[("AIDC_ISSUER", &server.base)],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("belongs to another application"));
    // The file is untouched after a rejected link.
    let document: serde_json::Value =
        serde_json::from_str(env.read(".aid.json").trim()).expect("json");
    assert!(document.get("client_id").is_none());
}

#[test]
fn sync_dry_run_previews_without_writing() {
    let server = MockServer::start(vec![
        Route::json(
            "GET",
            "/apps/a1",
            200,
            r#"{"id":"a1","name":"Renamed","description":"new desc"}"#,
        ),
        Route::json(
            "GET",
            "/clients/c1",
            200,
            r#"{"client_id":"c1","app_id":"a1","redirect_uris":["http://localhost:4000/cb"],"scopes":["openid","email"]}"#,
        ),
    ]);
    let env = Env::new("sync-dry");
    env.seed_tokens("token-1", 3_600_000, None);
    let original = format!(
        r#"{{"issuer":"{}","app_id":"a1","client_id":"c1","app_name":"Old","redirect_uri":"http://localhost:3000/cb","custom":"keep"}}"#,
        server.base
    );
    env.write(".aid.json", &original);

    let output = env.run(&["sync", "--dry-run"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(stdout.contains("SYNC PLAN"));
    assert!(stdout.contains("Application name: \"Old\" -> \"Renamed\""));
    assert!(stdout.contains("Dry run complete. No files were changed."));
    assert_eq!(env.read(".aid.json"), original, "dry run must not write");
}

#[test]
fn sync_applies_changes_and_preserves_unrelated_settings() {
    let server = MockServer::start(vec![
        Route::json(
            "GET",
            "/apps/a1",
            200,
            r#"{"id":"a1","name":"Renamed","description":"new desc"}"#,
        ),
        Route::json(
            "GET",
            "/clients/c1",
            200,
            r#"{"client_id":"c1","app_id":"a1","redirect_uris":["http://localhost:4000/cb"],"scopes":["openid","openid","email"]}"#,
        ),
    ]);
    let env = Env::new("sync-apply");
    env.seed_tokens("token-1", 3_600_000, None);
    env.write(
        ".aid.json",
        &format!(
            r#"{{"issuer":"{}","app_id":"a1","client_id":"c1","app_name":"Old","custom":"keep","nested":{{"a":1}}}}"#,
            server.base
        ),
    );

    let output = env.run(&["sync"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(stdout_of(&output).contains("Synced Renamed."));

    let document: serde_json::Value =
        serde_json::from_str(env.read(".aid.json").trim()).expect("json");
    assert_eq!(document["app_name"], "Renamed");
    assert_eq!(document["app_description"], "new desc");
    assert_eq!(document["redirect_uri"], "http://localhost:4000/cb");
    assert_eq!(document["scopes"], serde_json::json!(["openid", "email"]));
    assert_eq!(document["custom"], "keep");
    assert_eq!(document["nested"]["a"], 1);
    assert!(document["synced_at"].as_str().unwrap().ends_with('Z'));
}

#[test]
fn sync_reports_when_nothing_changed() {
    let server = MockServer::start(vec![Route::json(
        "GET",
        "/apps/a1",
        200,
        r#"{"id":"a1","name":"Alpha"}"#,
    )]);
    let env = Env::new("sync-noop");
    env.seed_tokens("token-1", 3_600_000, None);
    env.write(
        ".aid.json",
        &format!(
            r#"{{"issuer":"{}","app_id":"a1","app_name":"Alpha"}}"#,
            server.base
        ),
    );

    let output = env.run(&["sync"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(stdout_of(&output).contains("No metadata changes detected."));
}

#[test]
fn sync_refuses_other_issuers_and_missing_links() {
    let env = Env::new("sync-guard");
    env.seed_tokens("token-1", 3_600_000, None);

    let output = env.run(&["sync"], &[("AIDC_ISSUER", "http://127.0.0.1:9")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("No .aid.json found."));

    env.write(
        ".aid.json",
        r#"{"issuer":"http://127.0.0.1:1","app_id":"a1"}"#,
    );
    let output = env.run(&["sync"], &[("AIDC_ISSUER", "http://127.0.0.1:9")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("different issuer"));
}

#[test]
fn api_errors_are_labelled_and_redacted() {
    let server = MockServer::start(vec![Route::json(
        "GET",
        "/apps",
        400,
        r#"{"message":"bad access_token=super-secret-value for request"}"#,
    )]);
    let env = Env::new("api-error");
    env.seed_tokens("token-1", 3_600_000, None);

    let output = env.run(&["apps", "list"], &[("AIDC_ISSUER", &server.base)]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("Request failed (400): Bad request:"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("super-secret-value"),
        "secrets must be redacted"
    );
}

#[test]
fn request_timeouts_are_reported() {
    let server = MockServer::start(vec![
        Route::json("GET", "/apps", 200, "[]").delay(Duration::from_millis(1_500))
    ]);
    let env = Env::new("timeout");
    env.seed_tokens("token-1", 3_600_000, None);

    let output = env.run(
        &["apps", "list"],
        &[
            ("AIDC_ISSUER", &server.base),
            ("AIDC_REQUEST_TIMEOUT_MS", "300"),
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    // v1 renders `${ms / 1000}s`, so a 300 ms deadline reads as "0.3s".
    assert!(
        stderr_of(&output).contains("Request timed out after 0.3s."),
        "stderr: {:?}",
        stderr_of(&output)
    );
}

#[test]
fn redirects_are_never_followed() {
    let server = MockServer::start(vec![
        Route::json("GET", "/apps", 302, "").header("location", "http://evil.example/steal")
    ]);
    let env = Env::new("redirect");
    env.seed_tokens("token-1", 3_600_000, None);

    let output = env.run(&["apps", "list"], &[("AIDC_ISSUER", &server.base)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("redirected unexpectedly"));
}
