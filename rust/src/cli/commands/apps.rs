//! `aid apps` — application management.

use serde_json::{Map, Value};

use crate::api::client::{encode_segment, Method};
use crate::api::{api_fetch, api_get_json, api_json};
use crate::cli::args::{self, value};
use crate::cli::commands::list_options::{filter_resources, parse_list_options};
use crate::cli::commands::{cell, expect_object, expect_object_array, value_text};
use crate::config::Config;
use crate::error::{AidError, Result};
use crate::terminal::{command_list, fields, json_output, show_banner, success, table, usage};

const LIST_USAGE: &str = "Usage: aid apps list [--search <text>] [--limit <n>] [--json]";
const CREATE_USAGE: &str = "Usage: aid apps create --name <name> [--description <text>]";
const UPDATE_USAGE: &str = "Usage: aid apps update <id> [--name <name>] [--description <text>]";
const GET_USAGE: &str = "Usage: aid apps get <id> [--json]";
const DELETE_USAGE: &str = "Usage: aid apps delete <id> [<id> ...]";

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
        "update" => update(rest),
        "delete" => delete(rest),
        other => Err(AidError::new(format!("Unknown apps subcommand: {other}"))),
    }
}

fn print_help() {
    crate::terminal::clear_screen();
    show_banner(crate::cli::version());
    usage(
        "
USAGE

  aid apps <subcommand> [options]

SUBCOMMANDS
",
    );
    command_list(&[
        (
            "list [--search <text>] [--limit <n>] [--json]",
            "List/filter applications or emit JSON",
        ),
        ("create --name <name>", "Create an application"),
        ("get <id> [--json]", "Show one application"),
        ("update <id>", "Update an application"),
        ("delete <id>", "Delete an application"),
    ]);
}

fn list(argv: &[String]) -> Result<()> {
    let config = Config::from_env()?;
    let options = parse_list_options(argv, LIST_USAGE)?;
    let response = api_get_json(&config, "/apps")?;
    expect_object_array(
        &response,
        "The API returned an unexpected application list. Expected an array of application objects.",
    )?;

    let items: Vec<Value> = response
        .as_array()
        .map(|items| items.to_vec())
        .unwrap_or_default();
    let apps = filter_resources(&items, &options.search, options.limit, |app| {
        ["id", "name", "description"]
            .iter()
            .filter_map(|key| value_text(app.get(*key)))
            .collect()
    });

    if options.json {
        let projected: Vec<Value> = apps
            .iter()
            .map(|app| {
                let mut object = Map::new();
                for key in ["id", "name", "description", "created_at"] {
                    if let Some(value) = app.get(key) {
                        object.insert(key.to_string(), value.clone());
                    }
                }
                Value::Object(object)
            })
            .collect();
        json_output(&Value::Array(projected));
        return Ok(());
    }

    let rows: Vec<Vec<String>> = apps
        .iter()
        .map(|app| {
            let name = app
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| "(unnamed)".to_string());
            vec![cell(app.get("id")), name]
        })
        .collect();
    table(&["ID", "NAME"], &rows);
    Ok(())
}

fn create(argv: &[String]) -> Result<()> {
    let config = Config::from_env()?;
    let parsed = args::parse(
        argv,
        &[value("name", Some('n')), value("description", Some('d'))],
    )
    .map_err(|_| AidError::new(CREATE_USAGE))?;
    if !parsed.positionals().is_empty() {
        return Err(AidError::new(CREATE_USAGE));
    }

    let name = parsed
        .value("name")
        .ok_or_else(|| AidError::new("--name is required."))?
        .to_string();

    let mut body = Map::new();
    body.insert("name".to_string(), Value::String(name.clone()));
    if let Some(description) = parsed.value("description") {
        body.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }

    let app = api_json(&config, "/apps", Method::Post, Some(&Value::Object(body)))?;
    expect_object(&app, "The API returned an unexpected application response.")?;

    let created = app
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(&name)
        .to_string();
    success(&format!("Created {created}"));
    fields(&[("ID", value_text(app.get("id")))]);
    Ok(())
}

fn get(argv: &[String]) -> Result<()> {
    let config = Config::from_env()?;
    let Some((id, rest)) = argv.split_first() else {
        return Err(AidError::new(GET_USAGE));
    };
    // v1 accepts `--json` only as the second token.
    let as_json = match rest {
        [] => false,
        [flag] if flag == "--json" => true,
        _ => return Err(AidError::new(GET_USAGE)),
    };

    let app = api_get_json(&config, &format!("/apps/{}", encode_segment(id)))?;
    expect_object(&app, "The API returned an unexpected application response.")?;

    if as_json {
        let mut object = Map::new();
        for key in ["id", "name", "description", "created_at"] {
            if let Some(value) = app.get(key) {
                object.insert(key.to_string(), value.clone());
            }
        }
        json_output(&Value::Object(object));
        return Ok(());
    }

    fields(&[
        ("ID", value_text(app.get("id"))),
        ("Name", value_text(app.get("name"))),
        ("Description", value_text(app.get("description"))),
        ("Created", value_text(app.get("created_at"))),
    ]);
    Ok(())
}

fn update(argv: &[String]) -> Result<()> {
    let config = Config::from_env()?;
    let (id, rest) = match argv.split_first() {
        Some((id, rest)) => (Some(id.as_str()), rest),
        None => (None, &[][..]),
    };
    let Some(id) = id else {
        return Err(AidError::new(UPDATE_USAGE));
    };

    let parsed = args::parse(
        rest,
        &[value("name", Some('n')), value("description", Some('d'))],
    )
    .map_err(|_| AidError::new(UPDATE_USAGE))?;
    if !parsed.positionals().is_empty() {
        return Err(AidError::new(UPDATE_USAGE));
    }

    let mut body = Map::new();
    if let Some(name) = parsed.value("name") {
        body.insert("name".to_string(), Value::String(name.to_string()));
    }
    if let Some(description) = parsed.value("description") {
        body.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
    if body.is_empty() {
        return Err(AidError::new(
            "Nothing to update. Pass --name and/or --description.",
        ));
    }

    let app = api_json(
        &config,
        &format!("/apps/{}", encode_segment(id)),
        Method::Patch,
        Some(&Value::Object(body)),
    )?;
    let name = app
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| parsed.value("name").map(str::to_string))
        .unwrap_or_else(|| id.to_string());
    success(&format!("Updated {name}"));
    Ok(())
}

fn delete(argv: &[String]) -> Result<()> {
    let config = Config::from_env()?;
    let ids: Vec<&String> = argv.iter().filter(|id| !id.is_empty()).collect();
    if ids.is_empty() {
        return Err(AidError::new(DELETE_USAGE));
    }

    for id in ids {
        let response = api_fetch(
            &config,
            &format!("/apps/{}", encode_segment(id)),
            Method::Delete,
            None,
        )?;
        if response.status == 404 {
            return Err(AidError::new(format!("No app found with ID {id}")));
        }
        if !response.is_success() {
            return Err(AidError::new(format!(
                "Delete failed ({}) for {id}: {}",
                response.status,
                response.error_detail()
            )));
        }
        success(&format!("Deleted {id}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_subcommands_are_reported() {
        let err = run(&["nope".to_string()]).unwrap_err();
        assert_eq!(err.message(), "Unknown apps subcommand: nope");
    }

    #[test]
    fn delete_requires_an_id() {
        let err = delete(&[]).unwrap_err();
        assert_eq!(err.message(), DELETE_USAGE);
    }

    #[test]
    fn create_requires_a_name() {
        let err = create(&[]).unwrap_err();
        assert_eq!(err.message(), "--name is required.");
    }

    #[test]
    fn update_requires_something_to_change() {
        let err = update(&["app-1".to_string()]).unwrap_err();
        assert!(err.message().starts_with("Nothing to update"));
    }

    #[test]
    fn get_usage_is_enforced() {
        let err = get(&[]).unwrap_err();
        assert_eq!(err.message(), GET_USAGE);
        let err = get(&["app-1".to_string(), "--nope".to_string()]).unwrap_err();
        assert_eq!(err.message(), GET_USAGE);
    }
}
