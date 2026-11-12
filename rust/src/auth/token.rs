//! Secure token persistence with cross-process locking.
//!
//! Port of `src/auth/tokens.js`. On-disk format, file mode (0600), directory
//! mode (0700), atomic replacement and the advisory lock file all match v1 so an
//! existing `tokens.json` written by the Node CLI keeps working.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::config::{paths, storage};
use crate::error::{AidError, Result};
use crate::util;

const LOCK_WAIT: Duration = Duration::from_secs(30);
const STALE_LOCK: Duration = Duration::from_secs(60);
const LOCK_POLL: Duration = Duration::from_millis(50);
const INVALID_SESSION: &str = "Stored Ace ID session is invalid or unsafe. Run `aid login` again.";

/// Stored OAuth tokens.
///
/// Unknown fields written by another client are preserved through `extra`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Tokens {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    /// Issuer this session was created for.
    ///
    /// v1 ignores (and preserves) this key; v2 uses it to refuse using a session
    /// that belongs to a different `AIDC_ISSUER` instead of silently sending the
    /// wrong token to the wrong service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Tokens {
    pub fn new(access_token: impl Into<String>) -> Self {
        Self {
            access_token: access_token.into(),
            ..Default::default()
        }
    }

    /// True when this session may be used against `issuer`.
    ///
    /// Sessions written by v1 carry no issuer, so they are accepted (that keeps
    /// the shared store compatible); sessions written by v2 are pinned.
    pub fn belongs_to(&self, issuer: &str) -> bool {
        match &self.issuer {
            None => true,
            Some(stored) => stored.trim_end_matches('/') == issuer.trim_end_matches('/'),
        }
    }

    /// Validate and normalise, mirroring v1's `validateTokens`.
    pub fn validated(mut self) -> Result<Self> {
        let trimmed = self.access_token.trim().to_string();
        if trimmed.is_empty() {
            return Err(AidError::new("INVALID_TOKEN_DATA"));
        }
        self.access_token = trimmed;

        if let Some(expires_at) = self.expires_at {
            if expires_at <= 0 {
                return Err(AidError::new("INVALID_TOKEN_DATA"));
            }
        }
        Ok(self)
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// Parse a token document, validating types the same way v1 does.
    pub fn from_json(value: &Value) -> Result<Self> {
        let object = value
            .as_object()
            .ok_or_else(|| AidError::new("INVALID_TOKEN_DATA"))?;
        for key in [
            "access_token",
            "refresh_token",
            "id_token",
            "token_type",
            "scope",
        ] {
            if let Some(field) = object.get(key) {
                if !field.is_string() {
                    return Err(AidError::new("INVALID_TOKEN_DATA"));
                }
            }
        }
        if let Some(expires_at) = object.get("expires_at") {
            match expires_at.as_i64() {
                Some(value) if value > 0 => {}
                Some(_) => return Err(AidError::new("INVALID_TOKEN_DATA")),
                None => {
                    // Accept integral floats (e.g. 1.7e12) but reject fractions.
                    let number = expires_at
                        .as_f64()
                        .filter(|value| value.is_finite() && *value > 0.0 && value.fract() == 0.0)
                        .ok_or_else(|| AidError::new("INVALID_TOKEN_DATA"))?;
                    if number > i64::MAX as f64 {
                        return Err(AidError::new("INVALID_TOKEN_DATA"));
                    }
                }
            }
        }

        let tokens: Tokens = serde_json::from_value(value.clone())
            .map_err(|_| AidError::new("INVALID_TOKEN_DATA"))?;
        tokens.validated()
    }
}

fn tokens_path() -> Result<PathBuf> {
    paths::tokens_file()
}

fn lock_path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_os_string();
    name.push(".lock");
    PathBuf::from(name)
}

/// True when the token is missing, or expires within the refresh leeway.
pub fn is_expiring_soon(tokens: &Tokens, leeway_ms: i64) -> bool {
    match tokens.expires_at {
        None => true,
        Some(expires_at) => util::now_ms() + leeway_ms >= expires_at,
    }
}

/// Load tokens, returning `None` when the user is not signed in.
pub fn load() -> Result<Option<Tokens>> {
    load_from(&tokens_path()?)
}

pub fn load_from(file: &Path) -> Result<Option<Tokens>> {
    if let Some(dir) = file.parent() {
        storage::harden_dir(dir)?;
    }
    let raw = match storage::read_to_string_checked(file) {
        Ok(Some(raw)) => raw,
        Ok(None) => return Ok(None),
        Err(err) if err.message() == "INVALID_FILE_TYPE" => {
            return Err(AidError::new(INVALID_SESSION))
        }
        Err(err) => return Err(err),
    };
    let value: Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => return Err(AidError::new(INVALID_SESSION)),
    };
    match Tokens::from_json(&value) {
        Ok(tokens) => {
            storage::harden_file(file, storage::PRIVATE_FILE_MODE)?;
            Ok(Some(tokens))
        }
        Err(_) => Err(AidError::new(INVALID_SESSION)),
    }
}

/// Save tokens while holding the cross-process lock.
pub fn save(tokens: Tokens) -> Result<()> {
    with_token_lock(|| save_unlocked(tokens.clone()))
}

/// Save tokens assuming the caller already holds the lock.
pub fn save_unlocked(tokens: Tokens) -> Result<()> {
    let tokens = tokens.validated()?;
    let file = tokens_path()?;
    let dir = file
        .parent()
        .ok_or_else(|| AidError::new("Token path has no parent directory."))?
        .to_path_buf();
    storage::ensure_private_dir(&dir)?;
    storage::harden_dir(&dir)?;
    storage::write_json_atomic(&file, &tokens.to_json(), false, storage::PRIVATE_FILE_MODE)
}

/// Delete the stored session while holding the lock.
pub fn clear() -> Result<()> {
    with_token_lock(clear_unlocked)
}

/// Delete the stored session assuming the caller already holds the lock.
pub fn clear_unlocked() -> Result<()> {
    let file = tokens_path()?;
    match fs::remove_file(&file) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// Clear the session only when it still holds `expected_access_token`.
///
/// Prevents a 401 from a stale request deleting a session another process has
/// already refreshed.
pub fn clear_if_access_token(expected: &str) -> Result<bool> {
    with_token_lock(|| {
        let current = load()?;
        match current {
            Some(tokens) if util::constant_time_eq(&tokens.access_token, expected) => {
                clear_unlocked()?;
                Ok(true)
            }
            _ => Ok(false),
        }
    })
}

/// Run `callback` while holding the advisory session lock.
pub fn with_token_lock<T>(callback: impl FnOnce() -> Result<T>) -> Result<T> {
    let file = tokens_path()?;
    let dir = file
        .parent()
        .ok_or_else(|| AidError::new("Token path has no parent directory."))?
        .to_path_buf();
    storage::ensure_private_dir(&dir)?;
    storage::harden_dir(&dir)?;
    let lock = lock_path(&file);

    let started = Instant::now();
    let handle = loop {
        match acquire(&lock) {
            Ok(handle) => break handle,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                remove_stale_lock(&lock)?;
                if started.elapsed() >= LOCK_WAIT {
                    return Err(AidError::new(
                        "Timed out waiting for the Ace ID session lock. If no other aid process is running, remove the stale session lock and retry.",
                    ));
                }
                std::thread::sleep(LOCK_POLL);
            }
            Err(err) => return Err(err.into()),
        }
    };

    let result = callback();
    drop(handle);
    let _ = fs::remove_file(&lock);
    result
}

fn acquire(lock: &Path) -> std::io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(storage::PRIVATE_FILE_MODE);
    }
    let mut file = options.open(lock)?;
    let payload = format!(
        "{{\"pid\":{},\"created_at\":{}}}",
        std::process::id(),
        util::now_ms()
    );
    file.write_all(payload.as_bytes())?;
    file.sync_all()?;
    Ok(file)
}

fn remove_stale_lock(lock: &Path) -> Result<()> {
    let metadata = match fs::metadata(lock) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };

    let owner_pid = fs::read_to_string(lock)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| value.get("pid").and_then(Value::as_i64));

    match owner_pid {
        Some(pid) if pid > 0 => {
            if process_is_alive(pid as u32) {
                return Ok(());
            }
        }
        _ => {
            let age = metadata
                .modified()
                .ok()
                .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                .unwrap_or_default();
            if age < STALE_LOCK {
                return Ok(());
            }
        }
    }

    // Re-check identity so a freshly acquired lock is not removed.
    let latest = match fs::metadata(lock) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    if lock_identity(&latest) == lock_identity(&metadata) {
        match fs::remove_file(lock) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

fn lock_identity(metadata: &fs::Metadata) -> (u64, Option<SystemTime>, u64) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        (metadata.ino(), metadata.modified().ok(), metadata.len())
    }
    #[cfg(not(unix))]
    {
        (0, metadata.modified().ok(), metadata.len())
    }
}

/// Liveness probe used for stale lock recovery.
pub fn process_is_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        use std::process::{Command, Stdio};
        match Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
        {
            Ok(output) if output.status.success() => true,
            // EPERM means the process exists but belongs to another user.
            Ok(output) => String::from_utf8_lossy(&output.stderr).contains("permitted"),
            Err(_) => false,
        }
    }
    #[cfg(windows)]
    {
        use std::process::{Command, Stdio};
        match Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        {
            Ok(output) => String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()),
            Err(_) => false,
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_access_token() {
        let value = serde_json::json!({});
        assert!(Tokens::from_json(&value).is_err());
        let value = serde_json::json!({"access_token": "   "});
        assert!(Tokens::from_json(&value).is_err());
    }

    #[test]
    fn rejects_wrong_field_types() {
        let value = serde_json::json!({"access_token": "a", "refresh_token": 5});
        assert!(Tokens::from_json(&value).is_err());
        let value = serde_json::json!({"access_token": "a", "expires_at": "soon"});
        assert!(Tokens::from_json(&value).is_err());
        let value = serde_json::json!({"access_token": "a", "expires_at": -1});
        assert!(Tokens::from_json(&value).is_err());
    }

    #[test]
    fn accepts_and_trims_valid_documents() {
        let value = serde_json::json!({
            "access_token": "  token  ",
            "refresh_token": "refresh",
            "expires_at": 1_900_000_000_000i64
        });
        let tokens = Tokens::from_json(&value).unwrap();
        assert_eq!(tokens.access_token, "token");
        assert_eq!(tokens.refresh_token.as_deref(), Some("refresh"));
    }

    #[test]
    fn sessions_are_pinned_to_their_issuer() {
        let mut tokens = Tokens::new("a");
        assert!(
            tokens.belongs_to("https://identity.ace-base.cc"),
            "v1 sessions have no issuer"
        );
        tokens.issuer = Some("https://other.example".to_string());
        assert!(!tokens.belongs_to("https://identity.ace-base.cc"));
        assert!(tokens.belongs_to("https://other.example/"));
    }

    #[test]
    fn issuer_field_must_be_a_string() {
        let value = serde_json::json!({"access_token": "a", "issuer": 5});
        assert!(Tokens::from_json(&value).is_err());
    }

    #[test]
    fn unknown_fields_are_preserved() {
        let value = serde_json::json!({"access_token": "a", "custom": "keep-me"});
        let tokens = Tokens::from_json(&value).unwrap();
        assert_eq!(tokens.extra.get("custom").unwrap(), "keep-me");
    }

    #[test]
    fn expiry_uses_leeway() {
        let now = util::now_ms();
        let mut tokens = Tokens::new("a");
        assert!(
            is_expiring_soon(&tokens, 60_000),
            "missing expiry counts as expiring"
        );
        tokens.expires_at = Some(now + 120_000);
        assert!(!is_expiring_soon(&tokens, 60_000));
        tokens.expires_at = Some(now + 30_000);
        assert!(is_expiring_soon(&tokens, 60_000));
    }

    #[test]
    fn lock_is_exclusive_and_released() {
        // A second acquisition attempt fails while the first lock is held.
        let dir = std::env::temp_dir().join(format!("ace-lock-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let lock = dir.join("tokens.json.lock");
        let handle = acquire(&lock).unwrap();
        assert!(acquire(&lock).is_err());
        drop(handle);
        fs::remove_file(&lock).unwrap();
        assert!(acquire(&lock).is_ok());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn live_pid_is_detected() {
        assert!(process_is_alive(std::process::id()));
    }
}
