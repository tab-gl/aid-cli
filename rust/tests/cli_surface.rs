//! CLI surface tests: help, version, aliases, chaining and usage errors.
//!
//! These need no network and no session.

mod support;

use support::{stderr_of, stdout_of, Env};

#[test]
fn help_prints_the_command_reference() {
    let env = Env::new("help");
    let output = env.run(&["--help"], &[]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(stdout.contains("USAGE\n"), "help must use real newlines");
    assert!(stdout.contains("apps list"));
    assert!(stdout.contains("clients rotate-secret"));
    assert!(stdout.contains("COMMAND CHAINING"));
    assert!(!stdout.contains("\\n"), "help must not print literal \\n");
}

#[test]
fn bare_invocation_shows_help() {
    let env = Env::new("bare");
    let output = env.run(&[], &[]);
    assert!(output.status.success());
    assert!(stdout_of(&output).contains("USAGE"));
}

#[test]
fn version_reports_the_crate_version() {
    let env = Env::new("version");
    for flag in ["--version", "-v", "version"] {
        let output = env.run(&[flag], &[]);
        assert!(output.status.success(), "{flag} failed");
        let stdout = stdout_of(&output);
        assert!(
            stdout.trim() == format!("ace-id {}", env!("CARGO_PKG_VERSION")),
            "unexpected version output: {stdout}"
        );
    }
}

#[test]
fn unknown_commands_fail_with_a_useful_error() {
    let env = Env::new("unknown");
    let output = env.run(&["definitely-not-a-command"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("Unknown command: definitely-not-a-command"));
}

#[test]
fn unknown_subcommands_are_reported() {
    let env = Env::new("unknown-sub");
    let output = env.run(&["apps", "nope"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("Unknown apps subcommand: nope"));

    let output = env.run(&["clients", "nope"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("Unknown clients subcommand: nope"));
}

#[test]
fn subcommand_help_is_available() {
    let env = Env::new("sub-help");
    let output = env.run(&["apps", "help"], &[]);
    assert!(output.status.success());
    assert!(stdout_of(&output).contains("aid apps <subcommand>"));

    let output = env.run(&["clients", "--help"], &[]);
    assert!(output.status.success());
    assert!(stdout_of(&output).contains("aid clients <subcommand>"));
}

#[test]
fn binary_aliases_are_accepted_as_a_prefix() {
    let env = Env::new("alias");
    let output = env.run(&["ace-id", "--version"], &[]);
    assert!(output.status.success());
    assert!(stdout_of(&output).starts_with("ace-id "));

    let output = env.run(&["aidc", "--version"], &[]);
    assert!(output.status.success());
}

#[test]
fn commands_chain_and_stop_at_the_first_failure() {
    let env = Env::new("chain");
    let output = env.run(&["--version", ";", "--version"], &[]);
    assert!(output.status.success());
    assert_eq!(stdout_of(&output).matches("ace-id ").count(), 2);

    let output = env.run(&["nope", ";", "--version"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        !stdout_of(&output).contains("ace-id "),
        "chain must stop on failure"
    );
}

#[test]
fn usage_errors_match_v1_text() {
    let env = Env::new("usage");
    let cases: Vec<(Vec<&str>, &str)> = vec![
        (vec!["apps", "list", "extra"], "Usage: aid apps list"),
        (vec!["apps", "create"], "--name is required."),
        (vec!["apps", "get"], "Usage: aid apps get <id> [--json]"),
        (vec!["apps", "update", "app-1"], "Nothing to update."),
        (vec!["apps", "delete"], "Usage: aid apps delete <id>"),
        (vec!["clients", "create"], "--name is required."),
        (
            vec!["clients", "create", "--name", "x"],
            "At least one --redirect is required.",
        ),
        (
            vec!["clients", "delete"],
            "Usage: aid clients delete <client_id>",
        ),
        (vec!["link"], "Usage: aid link <app_id>"),
        (vec!["sync", "--dry-run", "app-1"], "Usage: aid sync"),
        (vec!["setup", "extra"], "Usage: aid setup"),
        (vec!["whoami", "--nope"], "Usage: aid whoami"),
        (
            vec!["apps", "list", "--limit", "0"],
            "--limit must be a positive integer.",
        ),
    ];
    for (args, expected) in cases {
        let output = env.run(&args, &[]);
        assert_eq!(output.status.code(), Some(1), "{args:?} should fail");
        let stderr = stderr_of(&output);
        assert!(
            stderr.contains(expected),
            "{args:?} should report {expected:?}, got {stderr:?}"
        );
    }
}

#[test]
fn help_flags_and_short_aliases_work() {
    let env = Env::new("help-flags");
    for flag in ["-h", "help", "h"] {
        let output = env.run(&[flag], &[]);
        assert!(output.status.success(), "{flag} failed");
        assert!(stdout_of(&output).contains("USAGE"));
    }
}

#[test]
fn setup_help_and_flag_parsing_are_strict() {
    let env = Env::new("setup-strict");
    let output = env.run(&["setup", "--unknown-flag"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("Usage: aid setup"));
}
