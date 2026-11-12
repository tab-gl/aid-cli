//! Platform-specific directories.
//!
//! v1 used `os.homedir()` plus `XDG_CONFIG_HOME` on POSIX and `APPDATA` on
//! Windows. v2 reproduces those rules without pulling in a `dirs` dependency.

use std::path::{Path, PathBuf};

use crate::error::{AidError, Result};

/// Best-effort home directory lookup.
pub fn home_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| {
                let drive = std::env::var_os("HOMEDRIVE")?;
                let path = std::env::var_os("HOMEPATH")?;
                let mut home = PathBuf::from(drive);
                home.push(path);
                Some(home)
            })
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// The `aidc` configuration directory holding `tokens.json`.
pub fn config_dir() -> Result<PathBuf> {
    config_dir_from(|key| std::env::var_os(key).map(PathBuf::from))
}

pub fn config_dir_from(lookup: impl Fn(&str) -> Option<PathBuf>) -> Result<PathBuf> {
    if cfg!(windows) {
        let base = lookup("APPDATA").or_else(home_dir).ok_or_else(|| {
            AidError::new(
                "Could not determine the configuration directory: no home directory found.",
            )
        })?;
        return Ok(base.join("aidc"));
    }

    let base = lookup("XDG_CONFIG_HOME")
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| home_dir().map(|home| home.join(".config")))
        .ok_or_else(|| {
            AidError::new(
                "Could not determine the configuration directory: no home directory found.",
            )
        })?;
    Ok(base.join("aidc"))
}

/// Path of the stored token file.
pub fn tokens_file() -> Result<PathBuf> {
    Ok(config_dir()?.join("tokens.json"))
}

/// Resolve `path` against `root` and reject anything that escapes `root`.
pub fn ensure_within(root: &Path, path: &Path) -> Result<PathBuf> {
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let normalized = normalize_lexically(&resolved);
    let root_normalized = normalize_lexically(root);
    if !normalized.starts_with(&root_normalized) {
        return Err(AidError::new(
            "Refusing to write outside the project directory.",
        ));
    }
    Ok(normalized)
}

/// Lexically normalise `.` and `..` without touching the filesystem.
pub fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_config_home_is_respected() {
        let dir =
            config_dir_from(|key| (key == "XDG_CONFIG_HOME").then(|| PathBuf::from("/tmp/xdg")))
                .expect("dir");
        if cfg!(windows) {
            // Windows uses APPDATA; skip the POSIX assertion.
            return;
        }
        assert_eq!(dir, PathBuf::from("/tmp/xdg/aidc"));
    }

    #[test]
    fn path_traversal_is_rejected() {
        let root = Path::new("/work/project");
        assert!(ensure_within(root, Path::new("../../etc/passwd")).is_err());
        assert!(ensure_within(root, Path::new("/etc/passwd")).is_err());
        assert_eq!(
            ensure_within(root, Path::new("libs/library-release.aar")).unwrap(),
            PathBuf::from("/work/project/libs/library-release.aar")
        );
    }

    #[test]
    fn lexical_normalisation_collapses_dot_segments() {
        assert_eq!(
            normalize_lexically(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
    }
}
