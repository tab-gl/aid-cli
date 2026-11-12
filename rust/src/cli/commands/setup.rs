//! `aid setup` — configure a project, creating the remote app and OAuth client.
//!
//! Port of `src/commands/setup.js`, including the recovery journal, drift
//! inspection of an existing `.aid.json`, wizard prompts, SDK installation and
//! cleanup of half-created remote resources.

use std::path::Path;

use serde_json::{Map, Value};

use crate::api::client::{encode_segment, Method};
use crate::api::{api_fetch, api_json};
use crate::auth::token;
use crate::cli::args::{self, flag, multi, value};
use crate::config::project_config::{self, CONFIG_FILE, RECOVERY_FILE};
use crate::config::{storage, Config};
use crate::error::{AidError, Result};
use crate::project::{
    declared_dependency_version, detect_project, installed_sdk_version, project_name, SdkMode,
};
use crate::sdk::{
    install_for_project, sha256_file, verify_android_artifact, ANDROID_SDK_ASSET, ANDROID_SDK_DIR,
    STATUS_INSTALL_FAILED, STATUS_NOT_APPLICABLE, STATUS_SKIPPED,
};
use crate::terminal::output::show_banner;
use crate::terminal::{confirm, dim, fields, heading, is_interactive, prompt, success, warn};

pub const USAGE: &str = "Usage: aid setup [--interactive] [--name <name>] [--redirect <uri>] \
[--scope <scope>...] [--force]";

const OPTIONS: [args::Opt; 7] = [
    value("name", Some('n')),
    value("redirect", Some('r')),
    multi("scope", Some('s')),
    flag("force", Some('f')),
    flag("interactive", Some('i')),
    flag("wizard", Some('w')),
    flag("recover", None),
];

const STEPS: [&str; 7] = [
    "Detect project",
    "Check authentication",
    "Configure options",
    "Create application",
    "Create OAuth client",
    "Install SDK",
    "Write project config",
];

const DEFAULT_REDIRECT: &str = "http://localhost:3000/callback";

/// Minimal step display used by the interactive wizard.
struct StepProgress {
    enabled: bool,
}

impl StepProgress {
    fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    fn start(&self, index: usize) {
        if self.enabled {
            println!(
                "  {} {}/{} {}",
                dim("›"),
                index + 1,
                STEPS.len(),
                dim(STEPS[index])
            );
        }
    }

    fn complete(&self, index: usize, message: &str) {
        if self.enabled {
            println!("  {} {} {message}", dim("✓"), dim(STEPS[index]));
        }
    }

    fn skip(&self, index: usize, message: &str) {
        if self.enabled {
            println!("  {} {} {message}", dim("–"), dim(STEPS[index]));
        }
    }

    fn fail(&self, message: &str) {
        if self.enabled {
            eprintln!("  {} {message}", dim("✗"));
        }
    }
}

pub fn run(argv: &[String]) -> Result<()> {
    let config = Config::from_env()?;
    let parsed = args::parse(argv, &OPTIONS).map_err(|_| AidError::new(USAGE))?;
    if !parsed.positionals().is_empty() {
        return Err(AidError::new(USAGE));
    }

    show_banner(crate::cli::version());

    let root = std::env::current_dir()
        .map_err(|err| AidError::new(format!("Could not inspect project directory: {err}")))?;
    let config_path = root.join(CONFIG_FILE);
    let recovery_path = root.join(RECOVERY_FILE);
    let force = parsed.flag("force");

    if parsed.flag("recover") {
        return recover_setup(&config, &config_path, &recovery_path, force);
    }
    if project_config::exists(&recovery_path)? {
        return Err(AidError::new(
            "An interrupted setup recovery record exists. Run `aid setup --recover` before starting another setup.",
        ));
    }

    let interactive = parsed.flag("interactive") || parsed.flag("wizard");
    if interactive && !is_interactive() {
        return Err(AidError::new(
            "Interactive setup requires a terminal. Use `aid setup` for the fast non-interactive flow.",
        ));
    }

    if project_config::exists(&config_path)? && !force {
        inspect_existing_setup(&config, &root, &config_path)?;
        if !interactive {
            return Ok(());
        }
        if !confirm(
            "An existing .aid.json was found. Replace its configuration and create new remote resources? [y/N] ",
        )? {
            println!("Setup cancelled.");
            return Ok(());
        }
    }

    let progress = StepProgress::new(interactive);

    progress.start(0);
    let mut project = match detect_project() {
        Ok(project) => {
            progress.complete(0, &format!("Project detected: {}", project.label));
            project
        }
        Err(err) => {
            progress.fail("Project detection failed");
            return Err(err);
        }
    };

    heading(if interactive { "SETUP WIZARD" } else { "SETUP" });
    let sdk_note = if project.sdk.is_empty() {
        "native SDK not installed".to_string()
    } else {
        project.sdk.clone()
    };
    println!(
        "\n  {}  {}  •  {}\n",
        dim("DETECTED"),
        project.label,
        sdk_note
    );

    progress.start(1);
    let mut tokens = match token::load() {
        Ok(tokens) => tokens,
        Err(err) => {
            progress.fail("Could not read the saved session");
            return Err(err);
        }
    };
    if tokens.is_none() {
        if !is_interactive() {
            progress.fail("Authentication requires a terminal");
            return Err(AidError::new("Not signed in. Run `aid login` first."));
        }
        if let Err(err) = crate::cli::commands::login::run(&[]) {
            progress.fail("Authentication failed");
            return Err(err);
        }
        tokens = match token::load() {
            Ok(tokens) => tokens,
            Err(err) => {
                progress.fail("Could not read the saved session");
                return Err(err);
            }
        };
        if tokens.is_none() {
            progress.fail("Authentication failed");
            return Err(AidError::new("Sign-in completed without a usable session."));
        }
    }
    progress.complete(1, "Authentication ready");

    progress.start(2);
    let defaults = Settings {
        name: parsed
            .value("name")
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| project_name(&root)),
        redirect: parsed
            .value("redirect")
            .map(|redirect| redirect.trim().to_string())
            .filter(|redirect| !redirect.is_empty())
            .unwrap_or_else(|| DEFAULT_REDIRECT.to_string()),
        scopes: {
            let scopes = parsed.values("scope");
            if scopes.is_empty() {
                normalize_scopes(&config.scopes)
            } else {
                normalize_scopes(&scopes.join(" "))
            }
        },
    };

    let settings = if interactive {
        match collect_wizard_values(&defaults) {
            Ok(settings) => settings,
            Err(err) => {
                progress.fail("Configuration cancelled or invalid");
                return Err(err);
            }
        }
    } else {
        defaults
    };
    progress.complete(2, "Options confirmed");

    progress.start(3);
    let mut app_body = Map::new();
    app_body.insert("name".to_string(), Value::String(settings.name.clone()));
    let app = match api_json(
        &config,
        "/apps",
        Method::Post,
        Some(&Value::Object(app_body)),
    ) {
        Ok(app) => app,
        Err(err) => {
            progress.fail("Application creation failed");
            return Err(err);
        }
    };
    let app_id = match app.get("id").and_then(Value::as_str) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            progress.fail("Application response was invalid");
            return Err(AidError::new(
                "Application creation returned no usable ID. Check the Ace ID dashboard before retrying setup.",
            ));
        }
    };

    let mut recovery = Map::new();
    recovery.insert("issuer".to_string(), Value::String(config.issuer.clone()));
    recovery.insert("app_id".to_string(), Value::String(app_id.clone()));
    recovery.insert("client_id".to_string(), Value::Null);
    recovery.insert(
        "created_at".to_string(),
        Value::String(crate::util::now_iso8601()),
    );
    recovery.insert(
        "stage".to_string(),
        Value::String("app-created".to_string()),
    );

    if let Err(err) = project_config::write(&recovery_path, &recovery) {
        progress.fail("Could not record application recovery state");
        let failures = cleanup_remote_resources(&config, Some(&app_id), None);
        if !failures.is_empty() {
            warn(&format!(
                "Could not persist setup recovery state. Manually remove app {app_id}; cleanup also failed: {}",
                failures.join("; ")
            ));
        }
        return Err(AidError::new(format!(
            "Could not record setup recovery state: {}",
            err.message()
        )));
    }
    progress.complete(3, "Application created");

    progress.start(4);
    let mut client_body = Map::new();
    client_body.insert("name".to_string(), Value::String(settings.name.clone()));
    client_body.insert(
        "redirect_uris".to_string(),
        Value::Array(vec![Value::String(settings.redirect.clone())]),
    );
    client_body.insert(
        "scopes".to_string(),
        Value::Array(settings.scopes.iter().cloned().map(Value::String).collect()),
    );
    client_body.insert("public".to_string(), Value::Bool(true));

    let client = match api_json(
        &config,
        "/clients",
        Method::Post,
        Some(&Value::Object(client_body)),
    ) {
        Ok(client) => client,
        Err(err) => {
            progress.fail("OAuth client creation failed");
            return Err(finish_failed_setup(
                &config,
                &recovery_path,
                &app_id,
                None,
                err,
            ));
        }
    };
    let client_id = match client.get("client_id").and_then(Value::as_str) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            progress.fail("OAuth client creation failed");
            return Err(finish_failed_setup(
                &config,
                &recovery_path,
                &app_id,
                None,
                AidError::new("OAuth client creation returned no usable client ID."),
            ));
        }
    };

    recovery.insert("client_id".to_string(), Value::String(client_id.clone()));
    recovery.insert(
        "stage".to_string(),
        Value::String("resources-created".to_string()),
    );
    if let Err(err) = project_config::write(&recovery_path, &recovery) {
        progress.fail("Could not record OAuth client recovery state");
        let failures = cleanup_remote_resources(&config, Some(&app_id), Some(&client_id));
        if !failures.is_empty() {
            warn(&format!(
                "Could not record recovery state. Manually inspect app {app_id} and client {client_id}; cleanup failed: {}",
                failures.join("; ")
            ));
        } else {
            remove_recovery(&recovery_path);
        }
        return Err(AidError::new(format!(
            "Could not record setup recovery state: {}",
            err.message()
        )));
    }
    progress.complete(4, "OAuth client created");

    progress.start(5);
    let sdk_status = match install_for_project(&config, &root, &mut project) {
        Ok(status) => status,
        Err(err) => {
            warn(&format!("SDK setup skipped: {}", err.message()));
            STATUS_SKIPPED.to_string()
        }
    };
    if [STATUS_NOT_APPLICABLE, STATUS_SKIPPED, STATUS_INSTALL_FAILED].contains(&sdk_status.as_str())
    {
        progress.skip(5, &format!("SDK step: {sdk_status}"));
    } else {
        progress.complete(5, &format!("SDK step: {sdk_status}"));
    }

    progress.start(6);
    let mut document = Map::new();
    document.insert("issuer".to_string(), Value::String(config.issuer.clone()));
    document.insert("app_id".to_string(), Value::String(app_id.clone()));
    document.insert("client_id".to_string(), Value::String(client_id.clone()));
    document.insert(
        "redirect_uri".to_string(),
        Value::String(settings.redirect.clone()),
    );
    document.insert(
        "scopes".to_string(),
        Value::Array(settings.scopes.iter().cloned().map(Value::String).collect()),
    );
    document.insert(
        "project_type".to_string(),
        Value::String(project.kind.clone()),
    );
    document.insert(
        "framework".to_string(),
        Value::String(project.label.clone()),
    );
    document.insert("sdk".to_string(), Value::String(project.sdk.clone()));
    if let Some(path) = &project.sdk_path {
        document.insert("sdk_path".to_string(), Value::String(path.clone()));
    }
    if let Some(version) = &project.sdk_version {
        document.insert("sdk_version".to_string(), Value::String(version.clone()));
    }

    if let Err(err) = project_config::write(&config_path, &document) {
        progress.fail("Could not write project config");
        let failures = cleanup_remote_resources(&config, Some(&app_id), Some(&client_id));
        if !failures.is_empty() {
            recovery.insert(
                "stage".to_string(),
                Value::String("cleanup-required".to_string()),
            );
            let _ = project_config::write(&recovery_path, &recovery);
            warn("Setup cleanup is incomplete. Run aid setup --recover to retry removal of the created resources.");
        } else {
            remove_recovery(&recovery_path);
        }
        return Err(AidError::new(format!(
            "Could not write {CONFIG_FILE}: {}",
            err.message()
        )));
    }

    // If the journal cannot be removed, recovery recognises the completed
    // config and clears the journal without deleting live resources.
    if let Err(err) = std::fs::remove_file(&recovery_path) {
        if err.kind() != std::io::ErrorKind::NotFound {
            warn(&format!(
                "Setup succeeded, but the recovery record remains: {err}. Run aid setup --recover to clear it safely."
            ));
        }
    }
    progress.complete(6, "Project configuration saved");

    success("Ace ID project configured.");
    let mut rows: Vec<(&str, Option<String>)> = vec![
        ("Project", Some(project.label.clone())),
        ("Application ID", Some(app_id)),
        ("Client ID", Some(client_id)),
        ("Redirect URI", Some(settings.redirect)),
        ("Scopes", Some(settings.scopes.join(" "))),
        (
            "SDK",
            Some(if project.sdk.is_empty() {
                "none".to_string()
            } else {
                project.sdk.clone()
            }),
        ),
        ("SDK status", Some(sdk_status)),
    ];
    if let Some(path) = &project.sdk_path {
        rows.push(("SDK path", Some(path.clone())));
    }
    rows.push(("Config", Some(CONFIG_FILE.to_string())));
    fields(&rows);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub name: String,
    pub redirect: String,
    pub scopes: Vec<String>,
}

fn collect_wizard_values(defaults: &Settings) -> Result<Settings> {
    let name = prompt("Project name", &defaults.name)?;
    let redirect = prompt("Redirect URI", &defaults.redirect)?;
    let scopes = prompt("Scopes (space-separated)", &defaults.scopes.join(" "))?;
    if name.trim().is_empty() {
        return Err(AidError::new("Project name cannot be empty."));
    }
    if redirect.trim().is_empty() {
        return Err(AidError::new("Redirect URI cannot be empty."));
    }
    Ok(Settings {
        name: name.trim().to_string(),
        redirect: redirect.trim().to_string(),
        scopes: normalize_scopes(&scopes),
    })
}

/// Split, trim and de-duplicate a scope string.
pub fn normalize_scopes(scopes: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for scope in scopes.split_whitespace() {
        let trimmed = scope.trim();
        if !trimmed.is_empty() && !out.iter().any(|existing| existing == trimmed) {
            out.push(trimmed.to_string());
        }
    }
    out
}

fn finish_failed_setup(
    config: &Config,
    recovery_path: &Path,
    app_id: &str,
    client_id: Option<&str>,
    err: AidError,
) -> AidError {
    let failures = cleanup_remote_resources(config, Some(app_id), client_id);
    if failures.is_empty() {
        remove_recovery(recovery_path);
    } else {
        warn("Setup cleanup is incomplete. Run aid setup --recover to retry removal of the created resources.");
    }
    err
}

fn remove_recovery(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => warn(&format!(
            "Could not remove {RECOVERY_FILE}: {err}. Run aid setup --recover to clear it."
        )),
    }
}

/// Delete remote resources created by a failed setup. Returns failure notes.
fn cleanup_remote_resources(
    config: &Config,
    app_id: Option<&str>,
    client_id: Option<&str>,
) -> Vec<String> {
    let mut failures = Vec::new();

    if let Some(client_id) = client_id {
        match api_fetch(
            config,
            &format!("/clients/{}", encode_segment(client_id)),
            Method::Delete,
            None,
        ) {
            Ok(response) => {
                if !response.is_success() && response.status != 404 {
                    failures.push(format!("client {client_id} (HTTP {})", response.status));
                }
            }
            Err(err) => failures.push(format!("client {client_id} ({})", err.message())),
        }
    }
    if let Some(app_id) = app_id {
        match api_fetch(
            config,
            &format!("/apps/{}", encode_segment(app_id)),
            Method::Delete,
            None,
        ) {
            Ok(response) => {
                if !response.is_success() && response.status != 404 {
                    failures.push(format!("app {app_id} (HTTP {})", response.status));
                }
            }
            Err(err) => failures.push(format!("app {app_id} ({})", err.message())),
        }
    }

    for failure in &failures {
        warn(&format!("Cleanup failed for {failure}."));
    }
    failures
}

/// Inspect an existing `.aid.json` and report drift without touching anything.
fn inspect_existing_setup(config: &Config, root: &Path, config_path: &Path) -> Result<()> {
    let document = match project_config::read(config_path, CONFIG_FILE) {
        Ok(Some(document)) => document,
        Ok(None) => Map::new(),
        Err(err) => {
            if err.message().contains("invalid JSON") {
                return Err(AidError::new(
                    ".aid.json contains invalid JSON. Repair the file manually or back it up before using `aid setup --force`.",
                ));
            }
            return Err(err);
        }
    };

    let project = detect_project()?;
    let dependency_version = declared_dependency_version(root, &project.sdk);
    let installed_version = installed_sdk_version(root, &project.sdk);
    let mut issues: Vec<String> = Vec::new();

    if document.get("issuer").and_then(Value::as_str) != Some(config.issuer.as_str()) {
        issues.push("Issuer differs from the active CLI issuer.".to_string());
    }
    for (key, detected, label) in [
        ("project_type", Some(project.kind.clone()), "project type"),
        ("framework", Some(project.label.clone()), "framework"),
        ("sdk", Some(project.sdk.clone()), "SDK"),
    ] {
        let recorded = document.get(key).cloned().unwrap_or(Value::Null);
        let recorded_text = if recorded.is_null() {
            None
        } else {
            recorded.as_str().map(str::to_string)
        };
        if recorded_text != detected {
            issues.push(format!(
                "Detected {label} differs from .aid.json ({} -> {}).",
                recorded_text.unwrap_or_else(|| "unset".to_string()),
                detected.unwrap_or_else(|| "unset".to_string())
            ));
        }
    }

    match project.sdk_mode {
        SdkMode::Build => {
            if dependency_version.is_none() {
                issues.push(format!(
                    "SDK dependency {} is not declared in package.json.",
                    project.sdk
                ));
            }
            if installed_version.is_none() {
                issues.push(format!(
                    "SDK {} is declared/detected but is not installed in node_modules.",
                    project.sdk
                ));
            }
            match (&dependency_version, &installed_version) {
                (Some(declared), Some(installed)) => {
                    println!("  {}  {}: {declared}", dim("SDK RANGE"), project.sdk);
                    println!("  {}  Installed: {installed}", dim("SDK VERSION"));
                    if let Some(recorded) = document.get("sdk_version").and_then(Value::as_str) {
                        if recorded != installed {
                            issues.push(format!(
                                "Recorded SDK version {recorded} differs from installed {installed}."
                            ));
                        }
                    }
                    if sdk_range_compatibility(declared, installed) == Some(false) {
                        issues.push(format!(
                            "Installed SDK version {installed} does not satisfy declared range {declared}."
                        ));
                    }
                }
                _ => {
                    println!(
                        "  {}  {}: declared {}, installed {}",
                        dim("SDK VERSION"),
                        project.sdk,
                        dependency_version.as_deref().unwrap_or("no"),
                        installed_version.as_deref().unwrap_or("no")
                    );
                }
            }
        }
        SdkMode::Android => {
            let sdk_path = document
                .get("sdk_path")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{ANDROID_SDK_DIR}/{ANDROID_SDK_ASSET}"));
            let resolved = match crate::config::paths::ensure_within(root, Path::new(&sdk_path)) {
                Ok(path) => path,
                Err(_) => root.join(ANDROID_SDK_DIR).join(ANDROID_SDK_ASSET),
            };
            let present = resolved.is_file();
            let verified = present && verify_android_artifact(&resolved).unwrap_or(false);
            let detail = if !present {
                "missing".to_string()
            } else if verified {
                "1.1 (verified)".to_string()
            } else {
                "1.1 (checksum differs)".to_string()
            };
            println!("  {}  Android artifact: {detail}", dim("SDK VERSION"));
            if !present {
                issues.push("The configured Android SDK artifact is missing.".to_string());
            } else if !verified {
                issues.push(
                    "The installed Android SDK artifact does not match the pinned 1.1 checksum."
                        .to_string(),
                );
            }
            let _ = sha256_file;
        }
        SdkMode::Manual => {}
    }

    heading("EXISTING SETUP");
    fields(&[
        ("Project", Some(project.label.clone())),
        (
            "Application ID",
            Some(
                document
                    .get("app_id")
                    .and_then(Value::as_str)
                    .unwrap_or("(not linked)")
                    .to_string(),
            ),
        ),
        (
            "Client ID",
            Some(
                document
                    .get("client_id")
                    .and_then(Value::as_str)
                    .unwrap_or("(not linked)")
                    .to_string(),
            ),
        ),
        (
            "SDK",
            Some(if project.sdk.is_empty() {
                "none".to_string()
            } else {
                project.sdk.clone()
            }),
        ),
        ("Config", Some(CONFIG_FILE.to_string())),
    ]);

    if issues.is_empty() {
        success("No local project/configuration drift detected. Setup will not create duplicate remote resources.");
        println!("  To refresh remote app/client metadata, run: aid sync");
        println!("  Use aid setup --force only when you intentionally want to create a replacement setup.");
        return Ok(());
    }

    warn("Configuration drift detected:");
    for issue in &issues {
        println!("  • {issue}");
    }
    println!("\nMinimal repair suggestions:");
    let manager = project
        .package_manager
        .clone()
        .unwrap_or_else(|| "npm".to_string());
    if issues.iter().any(|issue| {
        issue.contains("is not declared") || issue.contains("does not satisfy declared range")
    }) {
        println!(
            "  • Install the existing SDK dependency with {manager} add {}.",
            project.sdk
        );
    }
    if issues
        .iter()
        .any(|issue| issue.contains("not installed in node_modules"))
    {
        println!("  • Restore declared dependencies with {manager} install.");
    }
    if issues.iter().any(|issue| {
        issue.contains("project type")
            || issue.contains("framework")
            || issue.contains("SDK differs")
            || issue.contains("Recorded SDK version")
    }) {
        println!(
            "  • Refresh local project metadata without creating a new app: aid link {} --force",
            document
                .get("app_id")
                .and_then(Value::as_str)
                .unwrap_or("<app-id>")
        );
    }
    if issues.iter().any(|issue| issue.contains("Issuer differs")) {
        println!("  • Confirm AIDC_ISSUER is set to the issuer that owns this project before syncing or relinking.");
    }
    if issues.iter().any(|issue| {
        issue.contains("Android SDK artifact is missing") || issue.contains("checksum differs")
    }) {
        println!(
            "  • Reinstall the pinned Android SDK artifact after confirming the target project."
        );
    }
    if !issues.iter().any(|issue| {
        issue.contains("SDK dependency")
            || issue.contains("node_modules")
            || issue.contains("project type")
            || issue.contains("framework")
            || issue.contains("SDK differs")
            || issue.contains("Recorded SDK version")
            || issue.contains("Issuer differs")
            || issue.contains("Android SDK artifact")
    }) {
        println!("  • Run aid sync to refresh remote metadata; existing remote resources will not be recreated.");
    }
    Ok(())
}

/// v1's semver range check for `^x.y.z`, `~x.y.z` and exact versions.
pub fn sdk_range_compatibility(range: &str, version: &str) -> Option<bool> {
    let (operator, expected) = parse_semver(range)?;
    let (_, actual) = parse_semver(version)?;
    Some(match operator.as_str() {
        "^" => {
            if expected.0 > 0 {
                actual.0 == expected.0
            } else if expected.1 > 0 {
                actual.0 == 0 && actual.1 == expected.1
            } else {
                actual.0 == 0 && actual.1 == 0 && actual.2 == expected.2
            }
        }
        "~" => actual.0 == expected.0 && actual.1 == expected.1,
        _ => actual == expected,
    })
}

fn parse_semver(value: &str) -> Option<(String, (u64, u64, u64))> {
    let trimmed = value.trim();
    let (operator, rest) = if let Some(rest) = trimmed.strip_prefix('^') {
        ("^".to_string(), rest)
    } else if let Some(rest) = trimmed.strip_prefix('~') {
        ("~".to_string(), rest)
    } else if let Some(rest) = trimmed.strip_prefix('=') {
        (String::new(), rest)
    } else {
        (String::new(), trimmed)
    };
    let rest = rest.strip_prefix('v').unwrap_or(rest);
    let mut parts = rest.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((operator, (major, minor, patch)))
}

fn recover_setup(
    config: &Config,
    config_path: &Path,
    recovery_path: &Path,
    force: bool,
) -> Result<()> {
    let raw = match storage::read_to_string(recovery_path)? {
        Some(raw) => raw,
        None => {
            println!("No interrupted setup recovery record found.");
            return Ok(());
        }
    };
    let recovery: Value = serde_json::from_str(&raw).map_err(|_| {
        AidError::new(format!(
            "Setup recovery record is invalid. Inspect {RECOVERY_FILE} before retrying."
        ))
    })?;

    let issuer_ok = recovery.get("issuer").and_then(Value::as_str) == Some(config.issuer.as_str());
    let app_id = recovery
        .get("app_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    let client_id = match recovery.get("client_id") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(client_id)) if !client_id.is_empty() => Ok(Some(client_id.clone())),
        _ => Err(()),
    };
    let stage_ok = matches!(
        recovery.get("stage").and_then(Value::as_str),
        Some("app-created" | "resources-created" | "cleanup-required")
    );
    let (Some(app_id), Ok(client_id)) = (app_id, client_id) else {
        return Err(AidError::new(format!(
            "Setup recovery record is invalid or belongs to another issuer. Inspect {RECOVERY_FILE} manually."
        )));
    };
    if !issuer_ok || !stage_ok {
        return Err(AidError::new(format!(
            "Setup recovery record is invalid or belongs to another issuer. Inspect {RECOVERY_FILE} manually."
        )));
    }

    let existing = match project_config::read(config_path, CONFIG_FILE) {
        Ok(existing) => existing,
        Err(_) => {
            return Err(AidError::new(format!(
                "Could not safely inspect {CONFIG_FILE}; recovery was not performed."
            )))
        }
    };

    let completed = existing
        .as_ref()
        .map(|document| {
            document.get("issuer").and_then(Value::as_str) == Some(config.issuer.as_str())
                && document.get("app_id").and_then(Value::as_str) == Some(app_id.as_str())
                && match &client_id {
                    None => true,
                    Some(client_id) => {
                        document.get("client_id").and_then(Value::as_str)
                            == Some(client_id.as_str())
                    }
                }
        })
        .unwrap_or(false);
    if completed {
        remove_recovery(recovery_path);
        success("Setup configuration is already complete; cleared the recovery record without deleting resources.");
        return Ok(());
    }

    if !force {
        if !is_interactive() {
            return Err(AidError::new(
                "Recovery can delete remote resources. Review the recovery record and rerun `aid setup --recover --force` in a non-interactive environment.",
            ));
        }
        let target = match &client_id {
            Some(client_id) => format!(
                "Remove app {app_id} and client {client_id} left by interrupted setup? [y/N] "
            ),
            None => format!("Remove app {app_id} left by interrupted setup? [y/N] "),
        };
        if !confirm(&target)? {
            println!("Recovery cancelled.");
            return Ok(());
        }
    }

    let failures = cleanup_remote_resources(config, Some(&app_id), client_id.as_deref());
    if !failures.is_empty() {
        return Err(AidError::new(
            "Setup recovery is incomplete. Fix the reported cleanup errors and rerun aid setup --recover.",
        ));
    }
    remove_recovery(recovery_path);
    success("Removed resources left behind by the interrupted setup.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_are_normalised_and_deduplicated() {
        assert_eq!(
            normalize_scopes("openid  profile openid"),
            vec!["openid", "profile"]
        );
        assert!(normalize_scopes("   ").is_empty());
    }

    #[test]
    fn caret_ranges_match_v1_semantics() {
        assert_eq!(sdk_range_compatibility("^1.2.3", "1.9.0"), Some(true));
        assert_eq!(sdk_range_compatibility("^1.2.3", "2.0.0"), Some(false));
        assert_eq!(sdk_range_compatibility("^0.2.3", "0.2.9"), Some(true));
        assert_eq!(sdk_range_compatibility("^0.2.3", "0.3.0"), Some(false));
        assert_eq!(sdk_range_compatibility("^0.0.3", "0.0.3"), Some(true));
        assert_eq!(sdk_range_compatibility("^0.0.3", "0.0.4"), Some(false));
    }

    #[test]
    fn tilde_and_exact_ranges_match_v1_semantics() {
        assert_eq!(sdk_range_compatibility("~1.2.3", "1.2.9"), Some(true));
        assert_eq!(sdk_range_compatibility("~1.2.3", "1.3.0"), Some(false));
        assert_eq!(sdk_range_compatibility("1.2.3", "1.2.3"), Some(true));
        assert_eq!(sdk_range_compatibility("v1.2.3", "1.2.3"), Some(true));
        assert_eq!(sdk_range_compatibility("1.2.3", "1.2.4"), Some(false));
    }

    #[test]
    fn unsupported_ranges_return_none() {
        assert_eq!(sdk_range_compatibility(">=1.0.0", "1.0.0"), None);
        assert_eq!(sdk_range_compatibility("1.2", "1.2.0"), None);
    }

    #[test]
    fn setup_usage_is_enforced() {
        let err = run(&["extra".to_string()]).unwrap_err();
        assert_eq!(err.message(), USAGE);
    }

    #[test]
    fn settings_are_plain_data() {
        let settings = Settings {
            name: "app".to_string(),
            redirect: DEFAULT_REDIRECT.to_string(),
            scopes: vec!["openid".to_string()],
        };
        assert_eq!(settings.scopes.len(), 1);
    }
}
