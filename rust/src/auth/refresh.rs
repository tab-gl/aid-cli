//! Access-token refresh with in-process and cross-process coordination.
//!
//! Port of `src/auth/refresh.js`. Two layers stop concurrent refreshes:
//! * a process-wide mutex, so parallel threads share one refresh, and
//! * the on-disk session lock, so parallel CLI processes serialise, with a
//!   re-read after the lock is acquired to reuse another process's fresh token.

use std::sync::Mutex;

use crate::auth::oauth::{self, merge_tokens};
use crate::auth::token;
use crate::config::Config;
use crate::error::{AidError, Result};

static REFRESH_MUTEX: Mutex<()> = Mutex::new(());

const NOT_SIGNED_IN: &str = "Not signed in. Run `aid login` first.";
const NO_REFRESH_TOKEN: &str =
    "Session expired and no refresh token is stored. Run `aid login` again.";

/// Return a usable access token, refreshing it when it is close to expiry.
pub fn get_valid_access_token(config: &Config) -> Result<String> {
    let tokens = token::load()?.ok_or_else(|| AidError::new(NOT_SIGNED_IN))?;
    ensure_issuer(&tokens, config)?;
    if !token::is_expiring_soon(&tokens, config.refresh_leeway_ms) {
        return Ok(tokens.access_token);
    }

    let _guard = REFRESH_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let refreshed = token::with_token_lock(|| {
        // Another process may have refreshed while this one waited.
        let latest = token::load()?.ok_or_else(|| AidError::new(NOT_SIGNED_IN))?;
        ensure_issuer(&latest, config)?;
        if !token::is_expiring_soon(&latest, config.refresh_leeway_ms) {
            return Ok(latest);
        }
        let Some(refresh_token) = latest.refresh_token.clone() else {
            token::clear_unlocked()?;
            return Err(AidError::new(NO_REFRESH_TOKEN));
        };

        match oauth::refresh_tokens(config, &refresh_token) {
            Ok(response) => {
                let merged = merge_tokens(Some(&latest), &response, &config.issuer);
                token::save_unlocked(merged.clone())?;
                Ok(merged)
            }
            Err(err) => {
                if err
                    .message()
                    .starts_with("Refresh failed because the session is no longer valid")
                {
                    token::clear_unlocked()?;
                }
                Err(err)
            }
        }
    })?;

    Ok(refreshed.access_token)
}

/// Refuse a stored session that belongs to another issuer.
pub fn ensure_issuer(tokens: &token::Tokens, config: &Config) -> Result<()> {
    if tokens.belongs_to(&config.issuer) {
        return Ok(());
    }
    let stored = tokens.issuer.clone().unwrap_or_default();
    Err(AidError::new(format!(
        "The stored session belongs to {stored}. Run `aid login` to sign in to {}.",
        config.issuer
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_session_reports_not_signed_in() {
        // Uses the real user config dir, so only assert the message shape when
        // no session exists; otherwise the call must not panic.
        let config = Config::from_lookup(|_| None).unwrap();
        match get_valid_access_token(&config) {
            Ok(_) => {}
            Err(err) => assert!(
                err.message().contains("Not signed in")
                    || err.message().contains("invalid or unsafe"),
                "unexpected message: {err}"
            ),
        }
    }
}
