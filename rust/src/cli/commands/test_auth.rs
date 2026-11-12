//! `aid test-auth` — verify the stored session can reach the API.

use serde_json::{Map, Value};

use crate::api::api_get_json;
use crate::cli::commands::{first_string, json_only};
use crate::config::Config;
use crate::error::{AidError, Result};
use crate::terminal::{fields, json_output, success};

pub fn run(args: &[String]) -> Result<()> {
    let as_json = json_only(args, "Usage: aid test-auth [--json]")?;
    let config = Config::from_env()?;

    let user = api_get_json(&config, "/me")?;
    if !user.is_object() {
        return Err(AidError::new(
            "Authentication check failed: the identity endpoint returned an unexpected response.",
        ));
    }

    let account = first_string(&user, &["name", "preferred_username", "email", "sub"])
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AidError::new(
                "Authentication check failed: the identity endpoint did not return a recognizable account.",
            )
        })?;

    let subject = user.get("sub").cloned();

    if as_json {
        let mut result = Map::new();
        result.insert("status".to_string(), Value::String("ok".to_string()));
        result.insert("issuer".to_string(), Value::String(config.issuer.clone()));
        result.insert(
            "client_id".to_string(),
            Value::String(config.client_id.clone()),
        );
        result.insert("account".to_string(), Value::String(account.clone()));
        if let Some(subject) = &subject {
            result.insert("subject".to_string(), subject.clone());
        }
        json_output(&Value::Object(result));
        return Ok(());
    }

    success("Authentication test passed. The stored session can access the Ace ID API.");
    fields(&[
        ("Issuer", Some(config.issuer.clone())),
        ("Client ID", Some(config.client_id.clone())),
        ("Account", Some(account)),
        (
            "Subject",
            subject.and_then(|value| value.as_str().map(str::to_string)),
        ),
    ]);
    Ok(())
}
