//! Project-local configuration files (`.aid.json` and the setup recovery journal).

use std::path::Path;

use serde_json::{Map, Value};

use crate::config::storage;
use crate::error::{AidError, Result};

/// Project configuration written by `setup` and `link`.
pub const CONFIG_FILE: &str = ".aid.json";
/// Journal used to recover an interrupted `setup`.
pub const RECOVERY_FILE: &str = ".aid-setup-recovery.json";

/// Read a project JSON document, returning `None` when the file is absent.
pub fn read(path: &Path, name: &str) -> Result<Option<Map<String, Value>>> {
    let raw = match storage::read_to_string(path)? {
        Some(raw) => raw,
        None => return Ok(None),
    };
    let value: Value = serde_json::from_str(&raw).map_err(|_| {
        AidError::new(format!(
            "{name} contains invalid JSON. Fix it before continuing; the file was left untouched."
        ))
    })?;
    match value {
        Value::Object(object) => Ok(Some(object)),
        _ => Err(AidError::new(format!(
            "{name} must contain a JSON object. The file was left untouched."
        ))),
    }
}

/// Write a project JSON document atomically with mode 0600 and a trailing newline.
pub fn write(path: &Path, document: &Map<String, Value>) -> Result<()> {
    storage::write_json_atomic(
        path,
        &Value::Object(document.clone()),
        true,
        storage::PRIVATE_FILE_MODE,
    )
}

/// True when the file exists.
pub fn exists(path: &Path) -> Result<bool> {
    storage::exists(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str) -> std::path::PathBuf {
        let mut random = [0u8; 6];
        getrandom::getrandom(&mut random).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "ace-cfg-{tag}-{}-{}",
            std::process::id(),
            crate::util::hex(&random)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_and_reads_a_project_document() {
        let dir = fixture("roundtrip");
        let path = dir.join(CONFIG_FILE);
        let mut document = Map::new();
        document.insert("issuer".to_string(), Value::String("https://x".to_string()));
        write(&path, &document).unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.ends_with('\n'));
        assert_eq!(read(&path, CONFIG_FILE).unwrap().unwrap(), document);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn invalid_json_is_reported_and_left_alone() {
        let dir = fixture("invalid");
        let path = dir.join(CONFIG_FILE);
        std::fs::write(&path, "{ nope").unwrap();
        assert!(read(&path, CONFIG_FILE).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ nope");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn non_object_documents_are_rejected() {
        let dir = fixture("array");
        let path = dir.join(CONFIG_FILE);
        std::fs::write(&path, "[]").unwrap();
        assert!(read(&path, CONFIG_FILE).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_documents_read_as_none() {
        let dir = fixture("missing");
        assert!(read(&dir.join(CONFIG_FILE), CONFIG_FILE).unwrap().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
