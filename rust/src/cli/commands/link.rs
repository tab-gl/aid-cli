//! `aid link` — link an existing application to the current folder.

use serde_json::{Map, Value};

use crate::api::api_get_json;
use crate::api::client::encode_segment;
use crate::cli::args::{self, flag, value};
use crate::cli::commands::list_options::string_list;
use crate::cli::commands::non_empty;
use crate::config::project_config::{self, CONFIG_FILE};
use crate::config::Config;
use crate::error::{AidError, Result};
use crate::project::detect_project;
use crate::terminal::{fields, success};

pub const USAGE: &str = "Usage: aid link <app_id> [--client <client_id>] [--force]";

const OPTIONS: [args::Opt; 2] = [value("client", Some('c')), flag("force", Some('f'))];

pub fn run(argv: &[String]) -> Result<()> {
    let config = Config::from_env()?;
    let parsed = args::parse(argv, &OPTIONS).map_err(|_| AidError::new(USAGE))?;

    let positionals = parsed.positionals();
    if positionals.len() > 1 {
        return Err(AidError::new(USAGE));
    }
    let Some(app_id) = non_empty(positionals.first().map(String::as_str)).map(str::to_string)
    else {
        return Err(AidError::new(USAGE));
    };

    let app = api_get_json(&config, &format!("/apps/{}", encode_segment(&app_id)))?;
    let returned_id = app
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| AidError::new(format!("App {app_id} was returned without a valid ID.")))?;
    if returned_id != app_id {
        return Err(AidError::new(format!(
            "The API returned app {returned_id} instead of requested app {app_id}. No local changes were made."
        )));
    }
    let returned_id = returned_id.to_string();

    let root = std::env::current_dir()
        .map_err(|err| AidError::new(format!("Could not inspect project directory: {err}")))?;
    let config_path = root.join(CONFIG_FILE);
    let exists = project_config::exists(&config_path)?;
    if !parsed.flag("force") && exists {
        return Err(AidError::new(format!(
            "{CONFIG_FILE} already exists. Use \"aid link {app_id} --force\" to replace it."
        )));
    }

    let project = detect_project()?;
    let existing = project_config::read(&config_path, CONFIG_FILE)?;
    let same_app = existing
        .as_ref()
        .map(|existing| {
            existing.get("issuer").and_then(Value::as_str) == Some(config.issuer.as_str())
                && existing.get("app_id").and_then(Value::as_str) == Some(returned_id.as_str())
        })
        .unwrap_or(false);

    // App-specific settings survive a relink of the same app only; switching
    // apps must not silently keep the previous client or redirect URI.
    let mut document = if same_app {
        existing.clone().unwrap_or_default()
    } else {
        Map::new()
    };
    document.insert("issuer".to_string(), Value::String(config.issuer.clone()));
    document.insert("app_id".to_string(), Value::String(returned_id.clone()));

    let client_id = match non_empty(parsed.value("client")) {
        Some(client) => Some(client.to_string()),
        None if same_app => document
            .get("client_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        None => None,
    };
    match &client_id {
        Some(client) => {
            document.insert("client_id".to_string(), Value::String(client.clone()));
        }
        None => {
            document.remove("client_id");
        }
    }

    document.insert(
        "project_type".to_string(),
        Value::String(project.kind.clone()),
    );
    document.insert(
        "framework".to_string(),
        Value::String(project.label.clone()),
    );
    document.insert("sdk".to_string(), Value::String(project.sdk.clone()));

    if let Some(client_id) = &client_id {
        validate_client(&config, client_id, &returned_id)?;
    }

    project_config::write(&config_path, &document)?;

    let app_name = app
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .unwrap_or(&returned_id)
        .to_string();
    success(&format!("Linked {app_name} to this folder."));
    fields(&[
        ("Application ID", Some(returned_id.clone())),
        ("Application", Some(app_name)),
        ("Client ID", client_id),
        ("Config", Some(CONFIG_FILE.to_string())),
    ]);
    Ok(())
}

/// Ensure an OAuth client exists and belongs to `app_id`.
pub fn validate_client(config: &Config, client_id: &str, app_id: &str) -> Result<()> {
    let client = api_get_json(config, &format!("/clients/{}", encode_segment(client_id)))?;
    if !client.is_object() {
        return Err(AidError::new(
            "The OAuth client is invalid or belongs to another application. No local changes were made.",
        ));
    }
    if client.get("client_id").and_then(Value::as_str) != Some(client_id) {
        return Err(AidError::new(
            "The OAuth client is invalid or belongs to another application. No local changes were made.",
        ));
    }
    if let Some(client_app) = client.get("app_id").and_then(Value::as_str) {
        if client_app != app_id {
            return Err(AidError::new(
                "The OAuth client is invalid or belongs to another application. No local changes were made.",
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
                "The OAuth client returned invalid redirect URIs. No local changes were made.",
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
                "The OAuth client returned invalid scopes. No local changes were made.",
            ));
        }
    }
    // Touch the helper so unused-import warnings stay meaningful if the shape
    // of the check changes.
    let _ = string_list(client.get("redirect_uris"));
    Ok(())
}
