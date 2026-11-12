//! `aid sync` — reconcile local `.aid.json` metadata with the server.
//!
//! Port of `src/commands/sync.js`: read-only by default when it would change
//! anything, `--dry-run` for a pure preview, unrelated project settings are
//! preserved, and a concurrent local edit aborts the write.

use serde_json::{Map, Value};

use crate::api::api_get_json;
use crate::api::client::encode_segment;
use crate::cli::args::{self, flag};
use crate::config::project_config::{self, CONFIG_FILE};
use crate::config::{storage, Config};
use crate::error::{AidError, Result};
use crate::terminal::{fields, success};

pub const USAGE: &str = "Usage: aid sync [<app_id>] [--dry-run]";

const OPTIONS: [args::Opt; 1] = [flag("dry-run", None)];

pub fn run(argv: &[String]) -> Result<()> {
    let config = Config::from_env()?;
    let parsed = args::parse(argv, &OPTIONS).map_err(|_| AidError::new(USAGE))?;

    let positionals = parsed.positionals();
    let app_id = positionals.first().cloned();
    let dry_run = parsed.flag("dry-run");
    if positionals.len() > 1 || (app_id.is_some() && dry_run) {
        return Err(AidError::new(USAGE));
    }

    if let Some(app_id) = app_id {
        // Switching the app intentionally drops metadata of the old app.
        return crate::cli::commands::link::run(&[app_id, "--force".to_string()]);
    }

    let root = std::env::current_dir()
        .map_err(|err| AidError::new(format!("Could not inspect project directory: {err}")))?;
    let config_path = root.join(CONFIG_FILE);

    let snapshot_raw = match storage::read_to_string(&config_path)? {
        Some(raw) => raw,
        None => {
            return Err(AidError::new(
                "No .aid.json found. Run `aid link <app_id>` first.",
            ))
        }
    };
    let current: Map<String, Value> = match serde_json::from_str::<Value>(&snapshot_raw) {
        Ok(Value::Object(object)) => object,
        Ok(_) => {
            return Err(AidError::new(
                ".aid.json must contain a JSON object. The file was left untouched.",
            ))
        }
        Err(_) => return Err(AidError::new(
            ".aid.json contains invalid JSON. Fix it before syncing; the file was left untouched.",
        )),
    };

    if current.get("issuer").and_then(Value::as_str) != Some(config.issuer.as_str()) {
        return Err(AidError::new(
            "The linked project uses a different issuer. Refusing to sync it against the current account.",
        ));
    }
    let Some(linked_app_id) = current
        .get("app_id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(str::to_string)
    else {
        return Err(AidError::new(
            "The local .aid.json has no valid app_id. Run `aid link <app_id> --force`.",
        ));
    };

    let linked_client_id = match current.get("client_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(client_id)) if !client_id.trim().is_empty() => {
            Some(client_id.clone())
        }
        Some(_) => {
            return Err(AidError::new(
                "The local .aid.json contains an invalid client_id. Local configuration was not changed.",
            ))
        }
    };

    let app = api_get_json(
        &config,
        &format!("/apps/{}", encode_segment(&linked_app_id)),
    )?;
    let client = match &linked_client_id {
        Some(client_id) => Some(api_get_json(
            &config,
            &format!("/clients/{}", encode_segment(client_id)),
        )?),
        None => None,
    };

    if !app.is_object()
        || app
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .is_none()
    {
        return Err(AidError::new(
            "The linked application response was invalid. Local configuration was not changed.",
        ));
    }
    let remote_app_id = app
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if remote_app_id != linked_app_id {
        return Err(AidError::new(
            "The API returned a different application ID. Local configuration was not changed.",
        ));
    }
    for key in ["name", "description"] {
        if let Some(value) = app.get(key) {
            if !value.is_null() && !value.is_string() {
                return Err(AidError::new(format!(
                    "The linked application's {key} field was invalid. Local configuration was not changed."
                )));
            }
        }
    }

    if let (Some(client), Some(client_id)) = (&client, &linked_client_id) {
        let client_matches = client.is_object()
            && client.get("client_id").and_then(Value::as_str) == Some(client_id.as_str());
        if !client_matches {
            return Err(AidError::new(
                "The linked OAuth client response was invalid or belongs to another application. Local configuration was not changed.",
            ));
        }
        if let Some(client_app) = client.get("app_id").and_then(Value::as_str) {
            if client_app != remote_app_id {
                return Err(AidError::new(
                    "The linked OAuth client response was invalid or belongs to another application. Local configuration was not changed.",
                ));
            }
        }
        if let Some(uris) = client.get("redirect_uris") {
            if !uris
                .as_array()
                .map(|uris| uris.iter().all(Value::is_string))
                .unwrap_or(false)
            {
                return Err(AidError::new(
                    "The linked OAuth client returned invalid redirect URIs. Local configuration was not changed.",
                ));
            }
        }
        if let Some(scopes) = client.get("scopes") {
            if !scopes
                .as_array()
                .map(|scopes| scopes.iter().all(Value::is_string))
                .unwrap_or(false)
            {
                return Err(AidError::new(
                    "The linked OAuth client returned invalid scopes. Local configuration was not changed.",
                ));
            }
        }
    }

    // Only metadata the server actually supplied is rewritten; every other
    // local setting is carried over untouched.
    let mut next = current.clone();
    next.insert("app_id".to_string(), Value::String(remote_app_id.clone()));

    if let Some(name) = app.get("name") {
        match name.as_str().filter(|name| !name.trim().is_empty()) {
            Some(name) => {
                next.insert("app_name".to_string(), Value::String(name.to_string()));
            }
            None => {
                next.remove("app_name");
            }
        }
    }
    if let Some(description) = app.get("description") {
        match description
            .as_str()
            .filter(|description| !description.trim().is_empty())
        {
            Some(description) => {
                next.insert(
                    "app_description".to_string(),
                    Value::String(description.to_string()),
                );
            }
            None => {
                next.remove("app_description");
            }
        }
    }
    if let Some(client) = &client {
        if let Some(uris) = client.get("redirect_uris").and_then(Value::as_array) {
            match uris.first().and_then(Value::as_str) {
                Some(first) => {
                    next.insert("redirect_uri".to_string(), Value::String(first.to_string()));
                }
                None => {
                    next.remove("redirect_uri");
                }
            }
        }
        if let Some(scopes) = client.get("scopes").and_then(Value::as_array) {
            let mut deduped: Vec<String> = Vec::new();
            for scope in scopes.iter().filter_map(Value::as_str) {
                let trimmed = scope.trim();
                if !trimmed.is_empty() && !deduped.iter().any(|existing| existing == trimmed) {
                    deduped.push(trimmed.to_string());
                }
            }
            next.insert(
                "scopes".to_string(),
                Value::Array(deduped.into_iter().map(Value::String).collect()),
            );
        }
    }

    let changes = diff_metadata(&current, &next);

    println!("\nSYNC PLAN");
    if changes.is_empty() {
        println!("  No metadata changes detected. Local configuration is already up to date.");
        let app_name = app
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .unwrap_or(&remote_app_id)
            .to_string();
        let mut rows = vec![
            ("Application ID".to_string(), Some(remote_app_id.clone())),
            ("Application".to_string(), Some(app_name)),
        ];
        if let Some(client) = &client {
            rows.push((
                "Client ID".to_string(),
                client
                    .get("client_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            ));
        }
        rows.push(("Changes".to_string(), Some("none".to_string())));
        rows.push(("Config".to_string(), Some(CONFIG_FILE.to_string())));
        print_rows(&rows);
        return Ok(());
    }

    for change in &changes {
        println!("  {}: {} -> {}", change.field, change.before, change.after);
    }
    if dry_run {
        println!("\nDry run complete. No files were changed.");
        return Ok(());
    }

    // Refuse to clobber an edit made while the remote requests were in flight.
    let latest = storage::read_to_string(&config_path)?;
    if latest.as_deref() != Some(snapshot_raw.as_str()) {
        return Err(AidError::new(format!(
            "{CONFIG_FILE} changed while sync was running. No local changes were written; rerun the sync."
        )));
    }

    next.insert(
        "synced_at".to_string(),
        Value::String(crate::util::now_iso8601()),
    );
    project_config::write(&config_path, &next)?;

    let app_name = app
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .unwrap_or(&remote_app_id)
        .to_string();
    success(&format!("Synced {app_name}."));

    let mut rows = vec![
        ("Application ID".to_string(), Some(remote_app_id.clone())),
        ("Application".to_string(), Some(app_name)),
    ];
    if let Some(client) = &client {
        rows.push((
            "Client ID".to_string(),
            client
                .get("client_id")
                .and_then(Value::as_str)
                .map(str::to_string),
        ));
    }
    rows.push((
        "Changed fields".to_string(),
        Some(changes.len().to_string()),
    ));
    rows.push((
        "Project settings".to_string(),
        Some("preserved".to_string()),
    ));
    rows.push((
        "Last synced".to_string(),
        next.get("synced_at")
            .and_then(Value::as_str)
            .map(str::to_string),
    ));
    rows.push(("Config".to_string(), Some(CONFIG_FILE.to_string())));
    print_rows(&rows);
    Ok(())
}

fn print_rows(rows: &[(String, Option<String>)]) {
    let borrowed: Vec<(&str, Option<String>)> = rows
        .iter()
        .map(|(label, value)| (label.as_str(), value.clone()))
        .collect();
    fields(&borrowed);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataChange {
    pub field: String,
    pub before: String,
    pub after: String,
}

const DIFF_KEYS: [(&str, &str); 4] = [
    ("app_name", "Application name"),
    ("app_description", "Application description"),
    ("redirect_uri", "Redirect URI"),
    ("scopes", "Scopes"),
];

fn display(value: Option<&Value>) -> String {
    match value {
        None => "(unset)".to_string(),
        Some(value) => serde_json::to_string(value).unwrap_or_else(|_| "(unset)".to_string()),
    }
}

/// Compare only the metadata fields sync owns.
pub fn diff_metadata(
    current: &Map<String, Value>,
    next: &Map<String, Value>,
) -> Vec<MetadataChange> {
    let mut changes = Vec::new();
    for (key, label) in DIFF_KEYS {
        let before = current.get(key);
        let after = next.get(key);
        if before.is_none() && after.is_none() {
            continue;
        }
        if before == after {
            continue;
        }
        changes.push(MetadataChange {
            field: label.to_string(),
            before: display(before),
            after: display(after),
        });
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn identical_documents_have_no_changes() {
        let current = map(serde_json::json!({"app_name": "App", "scopes": ["openid"]}));
        let next = map(serde_json::json!({
            "app_name": "App",
            "scopes": ["openid"],
            "synced_at": "later"
        }));
        assert!(diff_metadata(&current, &next).is_empty());
    }

    #[test]
    fn changed_and_removed_fields_are_reported() {
        let current = map(serde_json::json!({"app_name": "Old", "redirect_uri": "http://a"}));
        let next = map(serde_json::json!({"app_name": "New"}));
        let changes = diff_metadata(&current, &next);
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].field, "Application name");
        assert_eq!(changes[0].before, "\"Old\"");
        assert_eq!(changes[0].after, "\"New\"");
        assert_eq!(changes[1].field, "Redirect URI");
        assert_eq!(changes[1].after, "(unset)");
    }

    #[test]
    fn added_fields_are_reported() {
        let current = map(serde_json::json!({}));
        let next = map(serde_json::json!({"scopes": ["openid", "email"]}));
        let changes = diff_metadata(&current, &next);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].before, "(unset)");
        assert_eq!(changes[0].after, "[\"openid\",\"email\"]");
    }

    #[test]
    fn usage_is_enforced() {
        let err = run(&["app".to_string(), "--dry-run".to_string()]).unwrap_err();
        assert_eq!(err.message(), USAGE);
        let err = run(&["a".to_string(), "b".to_string()]).unwrap_err();
        assert_eq!(err.message(), USAGE);
    }
}
