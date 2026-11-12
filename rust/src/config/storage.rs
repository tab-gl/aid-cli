//! Atomic, permission-hardened filesystem writes.
//!
//! Every JSON document the CLI owns (tokens, `.aid.json`, the setup recovery
//! journal) is written through here: temp file in the same directory, restrictive
//! mode, `fsync`, then `rename`, so a crash never leaves a half-written file.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

use serde_json::Value;

use crate::error::{AidError, Result};
use crate::util;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub const PRIVATE_FILE_MODE: u32 = 0o600;
pub const PRIVATE_DIR_MODE: u32 = 0o700;
pub const PUBLIC_FILE_MODE: u32 = 0o644;

/// Restrict a directory to the current user (no-op on Windows).
pub fn harden_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        match fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_DIR_MODE)) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Restrict a file to the given mode (no-op on Windows).
pub fn harden_file(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}

/// Create `path` (and parents) as a private directory.
pub fn ensure_private_dir(path: &Path) -> Result<()> {
    if cfg!(unix) {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        builder.mode(PRIVATE_DIR_MODE);
        builder.create(path)?;
    } else {
        fs::create_dir_all(path)?;
    }
    harden_dir(path)
}

fn temp_path_for(path: &Path) -> Result<std::path::PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| AidError::new("Refusing to write a file without a parent directory."))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AidError::new("Refusing to write a file with an invalid name."))?;
    let mut random = [0u8; 8];
    getrandom::getrandom(&mut random)
        .map_err(|err| AidError::new(format!("Secure random source unavailable: {err}")))?;
    Ok(parent.join(format!(
        "{name}.{}.{}.tmp",
        std::process::id(),
        util::hex(&random)
    )))
}

/// Write bytes atomically with the requested file mode.
pub fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let temp = temp_path_for(path)?;

    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(mode);
        }
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        harden_file(&temp, mode)?;
        fs::rename(&temp, path)?;
        harden_file(path, mode)?;
        sync_parent(path);
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Best-effort directory fsync so the rename itself is durable.
fn sync_parent(path: &Path) {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(handle) = File::open(parent) {
            let _ = handle.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Serialise `value` as pretty JSON and write it atomically.
///
/// v1 wrote `.aid.json` and the recovery journal with a trailing newline and
/// `tokens.json` without one; both forms are preserved for compatibility.
pub fn write_json_atomic(
    path: &Path,
    value: &Value,
    trailing_newline: bool,
    mode: u32,
) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value)?;
    if trailing_newline {
        text.push('\n');
    }
    write_atomic(path, text.as_bytes(), mode)
}

/// Read a UTF-8 file, returning `None` when it does not exist.
pub fn read_to_string(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(AidError::new(format!(
            "Could not read {}: {err}",
            display_name(path)
        ))),
    }
}

/// Read a UTF-8 file but refuse symlinks and non-regular files.
pub fn read_to_string_checked(path: &Path) -> Result<Option<String>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(AidError::new("INVALID_FILE_TYPE"));
    }
    Ok(Some(fs::read_to_string(path)?))
}

/// True when `path` exists (symlinks are not followed specially).
pub fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(AidError::new(format!(
            "Could not inspect {}: {err}",
            display_name(path)
        ))),
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let mut random = [0u8; 6];
        getrandom::getrandom(&mut random).expect("random");
        let dir = std::env::temp_dir().join(format!(
            "ace-id-storage-{tag}-{}-{}",
            std::process::id(),
            util::hex(&random)
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn write_atomic_replaces_content_and_leaves_no_temp_files() {
        let dir = temp_dir("atomic");
        let file = dir.join("tokens.json");
        write_json_atomic(&file, &json!({"a": 1}), false, PRIVATE_FILE_MODE).unwrap();
        write_json_atomic(&file, &json!({"a": 2}), false, PRIVATE_FILE_MODE).unwrap();

        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"a\": 2"));
        assert!(!text.ends_with('\n'));

        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temporary files must be cleaned up");
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn written_files_are_private() {
        let dir = temp_dir("mode");
        let file = dir.join("tokens.json");
        write_json_atomic(&file, &json!({"a": 1}), false, PRIVATE_FILE_MODE).unwrap();
        let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_secret_files_are_rejected() {
        let dir = temp_dir("symlink");
        let target = dir.join("real.json");
        fs::write(&target, "{}").unwrap();
        let link = dir.join("link.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_to_string_checked(&link).is_err());
        fs::remove_dir_all(&dir).ok();
    }
}
