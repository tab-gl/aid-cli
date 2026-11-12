//! Command implementations.

pub mod apps;
pub mod clients;
pub mod link;
pub mod list_options;
pub mod login;
pub mod logout;
pub mod setup;
pub mod sync;
pub mod test_auth;
pub mod whoami;

use serde_json::Value;

use crate::error::{AidError, Result};

/// Accept exactly `["--json"]` (or nothing) for commands that only support it.
///
/// Mirrors v1's `args.length === 1 && args[0] === "--json"` check.
pub fn json_only(args: &[String], usage: &str) -> Result<bool> {
    if args.is_empty() {
        return Ok(false);
    }
    if args.len() == 1 && args[0] == "--json" {
        return Ok(true);
    }
    Err(AidError::new(usage))
}

/// Convert a JSON value to the text v1 printed with `String(value)`.
pub fn value_text(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// Table cell rendering: missing values become an em dash, as in v1's table.
pub fn cell(value: Option<&Value>) -> String {
    value_text(value).unwrap_or_else(|| "—".to_string())
}

/// First non-empty string among `keys`, mirroring v1's `name || email || sub`.
pub fn first_string(object: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = object.get(*key).and_then(Value::as_str) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

/// Validate that a response is a JSON object.
pub fn expect_object(value: &Value, message: &str) -> Result<()> {
    if value.is_object() {
        Ok(())
    } else {
        Err(AidError::new(message))
    }
}

/// Validate that a response is an array of objects.
pub fn expect_object_array(value: &Value, message: &str) -> Result<()> {
    match value.as_array() {
        Some(items) if items.iter().all(Value::is_object) => Ok(()),
        _ => Err(AidError::new(message)),
    }
}
