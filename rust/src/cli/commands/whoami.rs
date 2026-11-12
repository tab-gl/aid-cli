//! `aid whoami` — show the signed-in account.

use serde_json::{Map, Value};

use crate::api::api_get_json;
use crate::cli::commands::{first_string, json_only};
use crate::config::Config;
use crate::error::{AidError, Result};
use crate::terminal::{fields, json_output};

pub fn run(args: &[String]) -> Result<()> {
    let as_json = json_only(args, "Usage: aid whoami [--json]")?;
    let config = Config::from_env()?;

    let user = api_get_json(&config, "/me")?;
    if !user.is_object() {
        return Err(AidError::new(
            "The identity endpoint returned an unexpected account response.",
        ));
    }

    let name = first_string(&user, &["name", "preferred_username", "email", "sub"]);
    let email = user.get("email").cloned();
    let subject = user.get("sub").cloned();

    if as_json {
        let mut result = Map::new();
        if let Some(name) = &name {
            result.insert("name".to_string(), Value::String(name.clone()));
        }
        if let Some(email) = &email {
            result.insert("email".to_string(), email.clone());
        }
        if let Some(subject) = &subject {
            result.insert("sub".to_string(), subject.clone());
        }
        json_output(&Value::Object(result));
        return Ok(());
    }

    fields(&[
        ("Signed in as", name),
        (
            "Email",
            email.and_then(|value| value.as_str().map(str::to_string)),
        ),
        (
            "Subject",
            subject.and_then(|value| value.as_str().map(str::to_string)),
        ),
    ]);
    Ok(())
}
