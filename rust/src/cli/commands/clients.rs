//! `aid clients` — OAuth client management.

use std::path::Path;

use serde_json::{Map, Value};

use crate::api::client::{encode_segment, Method};
use crate::api::{api_fetch, api_get_json, api_json};
use crate::cli::args::{self, flag, multi, value};
use crate::cli::commands::list_options::{filter_resources, parse_list_options, string_list};
use crate::cli::commands::{cell, expect_object, expect_object_array, value_text};
use crate::config::storage;
use crate::config::Config;
use crate::error::{AidError, Result};
use crate::terminal::{
    command_list, fields, json_output, show_banner, success, table, usage, warn,
};

const LIST_USAGE: &str = "Usage: aid clients list [--search <text>] [--limit <n>] [--json]";
const CREATE_USAGE: &str = "Usage: aid clients create --name <name> --redirect <uri> \
[--redirect <uri>...] [--scope <s>...] [--id <id>] [--public]";
const GET_USAGE: &str = "Usage: aid clients get <client_id> [--json]";
const DELETE_USAGE: &str = "Usage: aid clients delete <client_id>";
const ROTATE_USAGE: &str = "Usage: aid clients rotate-secret <client_id>";

const CONFIG_FILE: &str = ".aid.json";

pub fn run(argv: &[String]) -> Result<()> {
    let (sub, rest) = match argv.split_first() {
        Some((sub, rest)) => (sub.as_str(), rest),
        None => ("", &[][..]),
    };

    match sub {
        "" | "help" | "--help" => {
            print_help();
            Ok(())
        }
        "list" => list(rest),
        "create" => create(rest),
        "get" => get(rest),
        "delete" => delete(rest),
        "rotate-secret" => rotate_secret(rest),
        other => Err(AidError::new(format!(
            "Unknown clients subcommand: {other}"
        ))),
    }
}

fn print_help() {
    crate::terminal::clear_screen();
    show_banner(crate::cli::version());
    usage(
        "
USAGE

  aid clients <subcommand> [options]

SUBCOMMANDS
",
    );
    command_list(&[
        (
            "list [--search <text>] [--limit <n>] [--json]",
            "List/filter clients or emit JSON",
        ),
        ("create --name <n>", "Create a client"),
        ("get <client_id> [--json]", "Show one client"),
        ("delete <client_id>", "Delete a client"),
        ("rotate-secret <client_id>", "Rotate the secret"),
    ]);
}

/// Local link metadata read from `.aid.json`, if any.
struct LocalConfig {
    client_id: Option<String>,
    app_id: Option<String>,
    redirect_uri: Option<String>,
}

fn read_local_config(root: &Path) -> Result<Option<LocalConfig>> {
    let file = root.join(CONFIG_FILE);
    let Some(raw) = storage::read_to_string(&file).map_err(|err| {
        AidError::new(format!(
            "Could not inspect local .aid.json: {}",
            err.message()
        ))
    })?
    else {
        return Ok(None);
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return Ok(None);
    };
    if !value.is_object() {
        return Ok(None);
    }
    Ok(Some(LocalConfig {
        client_id: value
            .get("client_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        app_id: value
            .get("app_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        redirect_uri: value
            .get("redirect_uri")
            .and_then(Value::as_str)
            .map(str::to_string),
    }))
}

fn relationship_status(client: &Value, local: Option<&LocalConfig>, apps: &Value) -> String {
    let app_id = client.get("app_id").and_then(Value::as_str).unwrap_or("");
    let owner_exists = apps
        .as_array()
        .map(|items| {
            items.iter().any(|app| {
                app.get("id").and_then(Value::as_str) == Some(app_id) && !app_id.is_empty()
            })
        })
        .unwrap_or(false);

    let is_current = local
        .and_then(|local| local.client_id.as_deref())
        .zip(client.get("client_id").and_then(Value::as_str))
        .map(|(current, candidate)| current == candidate)
        .unwrap_or(false);

    let mislinked = is_current
        && local
            .and_then(|local| local.app_id.as_deref())
            .map(|local_app| !app_id.is_empty() && local_app != app_id)
            .unwrap_or(false);

    let redirect_mismatch = is_current
        && local
            .and_then(|local| local.redirect_uri.as_deref())
            .map(|redirect| {
                client
                    .get("redirect_uris")
                    .and_then(Value::as_array)
                    .map(|uris| !uris.iter().any(|uri| uri.as_str() == Some(redirect)))
                    .unwrap_or(false)
            })
            .unwrap_or(false);

    if mislinked {
        "MISLINKED".to_string()
    } else if app_id.is_empty() || !owner_exists {
        if app_id.is_empty() {
            "ORPHANED: no app".to_string()
        } else {
            "ORPHANED: app missing".to_string()
        }
    } else if redirect_mismatch {
        "REDIRECT MISMATCH".to_string()
    } else {
        "OK".to_string()
    }
}

fn list(argv: &[String]) -> Result<()> {
    let config = Config::from_env()?;
    let options = parse_list_options(argv, LIST_USAGE)?;

    let clients = api_get_json(&config, "/clients")?;
    expect_object_array(
        &clients,
        "The API returned an unexpected OAuth client list. Expected an array of client objects.",
    )?;
    let apps = api_get_json(&config, "/apps")?;
    expect_object_array(
        &apps,
        "The API returned an unexpected application list while resolving OAuth client ownership.",
    )?;

    let root = std::env::current_dir().unwrap_or_default();
    let local = read_local_config(&root)?;

    let app_name = |app_id: &str| -> String {
        apps.as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|app| app.get("id").and_then(Value::as_str) == Some(app_id))
            })
            .and_then(|app| app.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .unwrap_or_else(|| {
                if app_id.is_empty() {
                    "(unassigned)".to_string()
                } else {
                    app_id.to_string()
                }
            })
    };

    let items: Vec<Value> = clients
        .as_array()
        .map(|items| items.to_vec())
        .unwrap_or_default();

    let enriched: Vec<(Value, String, String)> = items
        .iter()
        .map(|client| {
            let app_id = client.get("app_id").and_then(Value::as_str).unwrap_or("");
            let name = app_name(app_id);
            let status = relationship_status(client, local.as_ref(), &apps);
            (client.clone(), name, status)
        })
        .collect();

    let filtered = filter_resources(
        &enriched,
        &options.search,
        options.limit,
        |(client, name, status)| {
            let mut values = vec![
                value_text(client.get("client_id")).unwrap_or_default(),
                value_text(client.get("name")).unwrap_or_default(),
                if client
                    .get("public")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    "public".to_string()
                } else {
                    "confidential".to_string()
                },
                value_text(client.get("app_id")).unwrap_or_default(),
                name.clone(),
                status.clone(),
            ];
            values.extend(string_list(client.get("redirect_uris")));
            values.extend(string_list(client.get("scopes")));
            values
        },
    );

    if options.json {
        let projected: Vec<Value> = filtered
            .iter()
            .map(|(client, name, status)| {
                let mut object = Map::new();
                for key in ["client_id", "name", "public", "app_id"] {
                    if let Some(value) = client.get(key) {
                        object.insert(key.to_string(), value.clone());
                    }
                }
                object.insert("application_name".to_string(), Value::String(name.clone()));
                object.insert(
                    "relationship_status".to_string(),
                    Value::String(status.clone()),
                );
                let is_current = local
                    .as_ref()
                    .and_then(|local| local.client_id.as_deref())
                    .zip(client.get("client_id").and_then(Value::as_str))
                    .map(|(current, candidate)| current == candidate)
                    .unwrap_or(false);
                let redirect_mismatch = is_current
                    && local
                        .as_ref()
                        .and_then(|local| local.redirect_uri.as_deref())
                        .map(|redirect| {
                            client
                                .get("redirect_uris")
                                .and_then(Value::as_array)
                                .map(|uris| !uris.iter().any(|uri| uri.as_str() == Some(redirect)))
                                .unwrap_or(false)
                        })
                        .unwrap_or(false);
                object.insert(
                    "redirect_uri_mismatch".to_string(),
                    Value::Bool(redirect_mismatch),
                );
                for key in ["redirect_uris", "scopes", "created_at"] {
                    if let Some(value) = client.get(key) {
                        object.insert(key.to_string(), value.clone());
                    }
                }
                Value::Object(object)
            })
            .collect();
        json_output(&Value::Array(projected));
        return Ok(());
    }

    let rows: Vec<Vec<String>> = filtered
        .iter()
        .map(|(client, name, status)| {
            let client_type = if client
                .get("public")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "public"
            } else {
                "confidential"
            };
            vec![
                cell(client.get("client_id")),
                cell(client.get("name")),
                client_type.to_string(),
                name.clone(),
                status.clone(),
            ]
        })
        .collect();
    table(
        &["CLIENT ID", "NAME", "TYPE", "APPLICATION", "RELATION"],
        &rows,
    );
    Ok(())
}

fn create(argv: &[String]) -> Result<()> {
    let config = Config::from_env()?;
    let parsed = args::parse(
        argv,
        &[
            value("name", Some('n')),
            multi("redirect", Some('r')),
            multi("scope", Some('s')),
            value("id", Some('i')),
            flag("public", Some('p')),
        ],
    )
    .map_err(|_| AidError::new(CREATE_USAGE))?;
    if !parsed.positionals().is_empty() {
        return Err(AidError::new(CREATE_USAGE));
    }

    let name = parsed
        .value("name")
        .ok_or_else(|| AidError::new("--name is required."))?
        .to_string();
    let redirects = parsed.values("redirect");
    if redirects.is_empty() {
        return Err(AidError::new("At least one --redirect is required."));
    }

    let mut body = Map::new();
    body.insert("name".to_string(), Value::String(name));
    body.insert(
        "redirect_uris".to_string(),
        Value::Array(redirects.into_iter().map(Value::String).collect()),
    );
    let scopes = parsed.values("scope");
    if !scopes.is_empty() {
        body.insert(
            "scopes".to_string(),
            Value::Array(scopes.into_iter().map(Value::String).collect()),
        );
    }
    if let Some(id) = parsed.value("id") {
        body.insert("client_id".to_string(), Value::String(id.to_string()));
    }
    if parsed.flag("public") {
        body.insert("public".to_string(), Value::Bool(true));
    }

    let result = api_json(
        &config,
        "/clients",
        Method::Post,
        Some(&Value::Object(body)),
    )?;
    expect_object(
        &result,
        "The API returned an unexpected OAuth client response.",
    )?;

    let created = result
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("client")
        .to_string();
    success(&format!("Created {created}"));

    let public = result
        .get("public")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    fields(&[
        ("Client ID", value_text(result.get("client_id"))),
        (
            "Type",
            Some(if public { "public" } else { "confidential" }.to_string()),
        ),
    ]);

    if let Some(secret) = result.get("client_secret").and_then(Value::as_str) {
        fields(&[("Client Secret", Some(secret.to_string()))]);
        warn("Save this secret now. It will not be shown again.");
        println!();
    }
    Ok(())
}

fn get(argv: &[String]) -> Result<()> {
    let config = Config::from_env()?;
    let (id, rest) = match argv.split_first() {
        Some((id, rest)) => (Some(id.as_str()), rest),
        None => (None, &[][..]),
    };
    let as_json = match rest {
        [] => false,
        [flag] if flag == "--json" => true,
        _ => return Err(AidError::new(GET_USAGE)),
    };
    let Some(id) = id else {
        return Err(AidError::new(GET_USAGE));
    };

    let client = api_get_json(&config, &format!("/clients/{}", encode_segment(id)))?;
    expect_object(
        &client,
        "The API returned an unexpected OAuth client response.",
    )?;

    if as_json {
        let mut object = Map::new();
        for key in [
            "client_id",
            "name",
            "public",
            "app_id",
            "redirect_uris",
            "scopes",
            "created_at",
        ] {
            if let Some(value) = client.get(key) {
                object.insert(key.to_string(), value.clone());
            }
        }
        json_output(&Value::Object(object));
        return Ok(());
    }

    let public = client
        .get("public")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    fields(&[
        ("Client ID", value_text(client.get("client_id"))),
        ("Name", value_text(client.get("name"))),
        (
            "Type",
            Some(if public { "public" } else { "confidential" }.to_string()),
        ),
        (
            "Redirect URIs",
            Some(string_list(client.get("redirect_uris")).join("\n")),
        ),
        ("Scopes", Some(string_list(client.get("scopes")).join(" "))),
        ("Created", value_text(client.get("created_at"))),
    ]);
    Ok(())
}

fn delete(argv: &[String]) -> Result<()> {
    let config = Config::from_env()?;
    let Some(id) = argv.first() else {
        return Err(AidError::new(DELETE_USAGE));
    };

    let response = api_fetch(
        &config,
        &format!("/clients/{}", encode_segment(id)),
        Method::Delete,
        None,
    )?;
    if response.status == 404 {
        return Err(AidError::new(format!("No client found with ID {id}")));
    }
    if !response.is_success() {
        return Err(AidError::new(format!(
            "Delete failed ({}): {}",
            response.status,
            response.error_detail()
        )));
    }
    success(&format!("Deleted {id}"));
    Ok(())
}

fn rotate_secret(argv: &[String]) -> Result<()> {
    let config = Config::from_env()?;
    let Some(id) = argv.first() else {
        return Err(AidError::new(ROTATE_USAGE));
    };

    let result = api_json(
        &config,
        &format!("/clients/{}/rotate-secret", encode_segment(id)),
        Method::Post,
        None,
    )?;
    expect_object(
        &result,
        "The API returned an unexpected OAuth client response.",
    )?;

    let rotated = result
        .get("client_id")
        .and_then(Value::as_str)
        .unwrap_or(id)
        .to_string();
    success(&format!("Rotated secret for {rotated}"));
    fields(&[("Client Secret", value_text(result.get("client_secret")))]);
    warn("Save this secret now. It will not be shown again.");
    warn("The previous secret is immediately invalid.");
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps(items: Value) -> Value {
        items
    }

    #[test]
    fn unknown_subcommands_are_reported() {
        let err = run(&["nope".to_string()]).unwrap_err();
        assert_eq!(err.message(), "Unknown clients subcommand: nope");
    }

    #[test]
    fn create_requires_redirects() {
        let err = create(&["--name".to_string(), "x".to_string()]).unwrap_err();
        assert_eq!(err.message(), "At least one --redirect is required.");
        let err = create(&[]).unwrap_err();
        assert_eq!(err.message(), "--name is required.");
    }

    #[test]
    fn status_reports_orphans_and_mismatches() {
        let client = serde_json::json!({"client_id": "c1", "app_id": ""});
        assert_eq!(
            relationship_status(&client, None, &apps(serde_json::json!([]))),
            "ORPHANED: no app"
        );

        let client = serde_json::json!({"client_id": "c1", "app_id": "a1"});
        assert_eq!(
            relationship_status(&client, None, &apps(serde_json::json!([]))),
            "ORPHANED: app missing"
        );

        let apps_list = apps(serde_json::json!([{"id": "a1", "name": "App"}]));
        assert_eq!(relationship_status(&client, None, &apps_list), "OK");
    }

    #[test]
    fn status_detects_mislink_and_redirect_mismatch() {
        let apps_list = apps(serde_json::json!([
            {"id": "a1", "name": "App"},
            {"id": "a2", "name": "Other"}
        ]));
        let local = LocalConfig {
            client_id: Some("c1".to_string()),
            app_id: Some("a2".to_string()),
            redirect_uri: Some("http://localhost:3000/callback".to_string()),
        };
        let client = serde_json::json!({
            "client_id": "c1",
            "app_id": "a1",
            "redirect_uris": ["http://localhost:3000/callback"]
        });
        assert_eq!(
            relationship_status(&client, Some(&local), &apps_list),
            "MISLINKED"
        );

        let local = LocalConfig {
            app_id: Some("a1".to_string()),
            redirect_uri: Some("http://localhost:4000/callback".to_string()),
            ..local
        };
        assert_eq!(
            relationship_status(&client, Some(&local), &apps_list),
            "REDIRECT MISMATCH"
        );
    }

    #[test]
    fn delete_and_rotate_require_ids() {
        assert_eq!(delete(&[]).unwrap_err().message(), DELETE_USAGE);
        assert_eq!(rotate_secret(&[]).unwrap_err().message(), ROTATE_USAGE);
    }
}
