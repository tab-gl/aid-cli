//! `aid setup` behaviour: fast path, drift inspection, recovery journal and
//! cleanup of half-created remote resources.

mod support;

use support::{stderr_of, stdout_of, Env, MockServer, Route};

fn app_created() -> Route {
    Route::json("POST", "/apps", 201, r#"{"id":"a1","name":"demo"}"#)
}

fn client_created() -> Route {
    Route::json(
        "POST",
        "/clients",
        201,
        r#"{"client_id":"c1","name":"demo"}"#,
    )
}

#[test]
fn setup_requires_a_session_in_non_interactive_environments() {
    let server = MockServer::start(vec![app_created()]);
    let env = Env::new("setup-anon");

    let output = env.run(&["setup"], &[("AIDC_ISSUER", &server.base)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("Not signed in. Run `aid login` first."));
    assert!(
        server.requests().is_empty(),
        "no remote work without a session"
    );
    assert!(!env.exists(".aid.json"));
}

#[test]
fn setup_creates_resources_and_writes_project_config() {
    let server = MockServer::start(vec![app_created(), client_created()]);
    let env = Env::new("setup");
    env.seed_tokens("at-1", 3_600_000, None);

    let output = env.run(&["setup"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(stdout.contains("Ace ID project configured."));
    assert!(stdout.contains("Vanilla JavaScript"));
    assert!(stdout.contains("not applicable"));

    let document: serde_json::Value =
        serde_json::from_str(env.read(".aid.json").trim()).expect("config json");
    assert_eq!(document["issuer"], server.base);
    assert_eq!(document["app_id"], "a1");
    assert_eq!(document["client_id"], "c1");
    assert_eq!(document["redirect_uri"], "http://localhost:3000/callback");
    assert_eq!(
        document["scopes"],
        serde_json::json!(["openid", "profile", "email", "offline_access"])
    );
    assert_eq!(document["project_type"], "vanilla");
    assert_eq!(document["sdk"], "ace-id-sdk");
    assert!(
        !env.exists(".aid-setup-recovery.json"),
        "journal must be cleared"
    );

    let client_request = &server.requests_to("POST", "/clients")[0];
    let body = client_request.json();
    assert_eq!(body["public"], true);
    assert_eq!(body["redirect_uris"][0], "http://localhost:3000/callback");
}

#[test]
fn setup_honours_name_redirect_and_scope_flags() {
    let server = MockServer::start(vec![app_created(), client_created()]);
    let env = Env::new("setup-flags");
    env.seed_tokens("at-1", 3_600_000, None);

    let output = env.run(
        &[
            "setup",
            "--name",
            "My App",
            "--redirect",
            "http://localhost:4000/cb",
            "--scope",
            "openid",
            "--scope",
            "email profile",
        ],
        &[("AIDC_ISSUER", &server.base)],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));

    let document: serde_json::Value =
        serde_json::from_str(env.read(".aid.json").trim()).expect("config json");
    assert_eq!(document["redirect_uri"], "http://localhost:4000/cb");
    assert_eq!(
        document["scopes"],
        serde_json::json!(["openid", "email", "profile"])
    );
    assert_eq!(
        server.requests_to("POST", "/apps")[0].json()["name"],
        "My App"
    );
}

#[test]
fn setup_skips_an_sdk_that_is_already_declared() {
    let server = MockServer::start(vec![
        Route::json("POST", "/apps", 201, r#"{"id":"a1","name":"demo"}"#),
        client_created(),
    ]);
    let env = Env::new("setup-sdk");
    env.seed_tokens("at-1", 3_600_000, None);
    env.write(
        "package.json",
        r#"{"name":"demo","dependencies":{"next":"14.0.0","ace-id-sdk":"1.0.0"}}"#,
    );

    let output = env.run(&["setup"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(stdout_of(&output).contains("already installed"));

    let document: serde_json::Value =
        serde_json::from_str(env.read(".aid.json").trim()).expect("config json");
    assert_eq!(document["project_type"], "next");
}

#[test]
fn setup_inspects_an_existing_config_without_creating_resources() {
    let server = MockServer::start(vec![app_created()]);
    let env = Env::new("setup-existing");
    env.seed_tokens("at-1", 3_600_000, None);
    let original = format!(
        r#"{{"issuer":"{}","app_id":"a1","client_id":"c1","project_type":"vanilla","framework":"Vanilla JavaScript","sdk":"ace-id-sdk"}}"#,
        server.base
    );
    env.write(".aid.json", &original);

    let output = env.run(&["setup"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(stdout.contains("EXISTING SETUP"));
    assert!(stdout.contains("No local project/configuration drift detected."));
    assert!(
        server.requests().is_empty(),
        "no remote resources may be created"
    );
    assert_eq!(env.read(".aid.json"), original, "config must be untouched");
}

#[test]
fn setup_reports_drift_and_suggests_repairs() {
    let server = MockServer::start(vec![app_created()]);
    let env = Env::new("setup-drift");
    env.seed_tokens("at-1", 3_600_000, None);
    env.write(
        ".aid.json",
        &format!(
            r#"{{"issuer":"{}","app_id":"a1","project_type":"react","framework":"React","sdk":"ace-id-sdk"}}"#,
            server.base
        ),
    );

    let output = env.run(&["setup"], &[("AIDC_ISSUER", &server.base)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    // Warnings go to stderr (as in v1); repair hints go to stdout.
    assert!(stderr_of(&output).contains("Configuration drift detected:"));
    let stdout = stdout_of(&output);
    assert!(stdout.contains("Minimal repair suggestions:"));
    assert!(stdout.contains("aid link a1 --force"));
}

#[test]
fn setup_refuses_to_run_while_a_recovery_journal_exists() {
    let server = MockServer::start(vec![app_created()]);
    let env = Env::new("setup-journal");
    env.seed_tokens("at-1", 3_600_000, None);
    env.write(
        ".aid-setup-recovery.json",
        &format!(
            r#"{{"issuer":"{}","app_id":"a9","client_id":null,"stage":"app-created"}}"#,
            server.base
        ),
    );

    let output = env.run(&["setup"], &[("AIDC_ISSUER", &server.base)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("interrupted setup recovery record exists"));
    assert!(server.requests().is_empty());
}

#[test]
fn setup_recover_removes_resources_from_an_interrupted_run() {
    let server = MockServer::start(vec![
        Route::json("DELETE", "/apps/a9", 204, ""),
        Route::json("DELETE", "/clients/c9", 204, ""),
    ]);
    let env = Env::new("setup-recover");
    env.seed_tokens("at-1", 3_600_000, None);
    env.write(
        ".aid-setup-recovery.json",
        &format!(
            r#"{{"issuer":"{}","app_id":"a9","client_id":"c9","stage":"resources-created"}}"#,
            server.base
        ),
    );

    let output = env.run(
        &["setup", "--recover", "--force"],
        &[("AIDC_ISSUER", &server.base)],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(stdout_of(&output).contains("Removed resources left behind"));
    assert!(!env.exists(".aid-setup-recovery.json"));
    assert_eq!(server.requests_to("DELETE", "/clients/c9").len(), 1);
    assert_eq!(server.requests_to("DELETE", "/apps/a9").len(), 1);
}

#[test]
fn setup_recover_keeps_resources_when_the_config_is_complete() {
    let server = MockServer::start(vec![Route::json("DELETE", "/apps/a9", 204, "")]);
    let env = Env::new("setup-recover-done");
    env.seed_tokens("at-1", 3_600_000, None);
    env.write(
        ".aid-setup-recovery.json",
        &format!(
            r#"{{"issuer":"{}","app_id":"a9","client_id":"c9","stage":"resources-created"}}"#,
            server.base
        ),
    );
    env.write(
        ".aid.json",
        &format!(
            r#"{{"issuer":"{}","app_id":"a9","client_id":"c9"}}"#,
            server.base
        ),
    );

    let output = env.run(
        &["setup", "--recover", "--force"],
        &[("AIDC_ISSUER", &server.base)],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(stdout_of(&output).contains("already complete"));
    assert!(!env.exists(".aid-setup-recovery.json"));
    assert!(
        server.requests().is_empty(),
        "live resources must not be deleted"
    );
}

#[test]
fn setup_recover_needs_force_without_a_terminal() {
    let server = MockServer::start(vec![Route::json("DELETE", "/apps/a9", 204, "")]);
    let env = Env::new("setup-recover-force");
    env.seed_tokens("at-1", 3_600_000, None);
    env.write(
        ".aid-setup-recovery.json",
        &format!(
            r#"{{"issuer":"{}","app_id":"a9","client_id":null,"stage":"app-created"}}"#,
            server.base
        ),
    );

    let output = env.run(&["setup", "--recover"], &[("AIDC_ISSUER", &server.base)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("--recover --force"));
    assert!(server.requests().is_empty());
}

#[test]
fn setup_cleans_up_the_app_when_client_creation_fails() {
    let server = MockServer::start(vec![
        app_created(),
        Route::json("POST", "/clients", 500, r#"{"message":"boom"}"#),
        Route::json("DELETE", "/apps/a1", 204, ""),
    ]);
    let env = Env::new("setup-cleanup");
    env.seed_tokens("at-1", 3_600_000, None);

    let output = env.run(&["setup"], &[("AIDC_ISSUER", &server.base)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("Request failed (500)"));
    assert_eq!(
        server.requests_to("DELETE", "/apps/a1").len(),
        1,
        "the created app must be removed"
    );
    assert!(!env.exists(".aid.json"));
    assert!(!env.exists(".aid-setup-recovery.json"));
}

#[test]
fn interactive_setup_requires_a_terminal() {
    let env = Env::new("setup-interactive");
    env.seed_tokens("at-1", 3_600_000, None);
    let output = env.run(
        &["setup", "--interactive"],
        &[("AIDC_ISSUER", "http://127.0.0.1:9")],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("Interactive setup requires a terminal"));
}

#[test]
fn setup_rejects_invalid_application_responses() {
    let server = MockServer::start(vec![Route::json(
        "POST",
        "/apps",
        201,
        r#"{"name":"no-id"}"#,
    )]);
    let env = Env::new("setup-bad-app");
    env.seed_tokens("at-1", 3_600_000, None);

    let output = env.run(&["setup"], &[("AIDC_ISSUER", &server.base)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("returned no usable ID"));
    assert!(!env.exists(".aid.json"));
}
