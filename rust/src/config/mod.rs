//! Runtime configuration resolved from the environment.
//!
//! Mirrors `src/auth/config.js` in v1, including validation messages and the
//! environment variables users already set.

pub mod paths;
pub mod project_config;
pub mod storage;

use std::time::Duration;

use url::Url;

use crate::error::{AidError, Result};

pub const DEFAULT_ISSUER: &str = "https://identity.ace-base.cc";
pub const DEFAULT_CLIENT_ID: &str = "aidc-cli";
pub const DEFAULT_CALLBACK_PORT: u16 = 7357;
pub const DEFAULT_SCOPES: &str = "openid profile email offline_access";
pub const DEFAULT_CALLBACK_TIMEOUT_MS: u64 = 5 * 60 * 1000;
pub const DEFAULT_REFRESH_LEEWAY_MS: i64 = 60 * 1000;
pub const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 15 * 1000;

#[derive(Debug, Clone)]
pub struct Config {
    /// Normalised issuer URL without a trailing slash.
    pub issuer: String,
    pub issuer_url: Url,
    pub client_id: String,
    pub callback_port: u16,
    pub redirect_uri: String,
    pub scopes: String,
    pub callback_timeout: Duration,
    pub refresh_leeway_ms: i64,
    pub request_timeout: Duration,
}

impl Config {
    /// Read configuration from the environment, applying v1 defaults.
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Testable constructor: `lookup` returns the raw value for a variable.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let issuer = normalize_issuer(
            lookup("AIDC_ISSUER")
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_ISSUER),
        )?;
        let issuer_url = Url::parse(&issuer)
            .map_err(|_| AidError::new("AIDC_ISSUER must be a valid absolute URL."))?;

        let client_id = lookup("AIDC_CLIENT_ID")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string());

        let callback_port = parse_port(lookup("AIDC_PORT").as_deref())?;
        let scopes = normalize_scopes(lookup("AIDC_SCOPES").as_deref())?;

        let callback_timeout_ms = parse_positive_int(
            lookup("AIDC_CALLBACK_TIMEOUT_MS").as_deref(),
            DEFAULT_CALLBACK_TIMEOUT_MS,
        )?;
        let refresh_leeway_ms = parse_positive_int(
            lookup("AIDC_REFRESH_LEEWAY_MS").as_deref(),
            DEFAULT_REFRESH_LEEWAY_MS as u64,
        )? as i64;
        let request_timeout_ms = parse_positive_int(
            lookup("AIDC_REQUEST_TIMEOUT_MS").as_deref(),
            DEFAULT_REQUEST_TIMEOUT_MS,
        )?;

        Ok(Self {
            redirect_uri: format!("http://127.0.0.1:{callback_port}/callback"),
            issuer,
            issuer_url,
            client_id,
            callback_port,
            scopes,
            callback_timeout: Duration::from_millis(callback_timeout_ms),
            refresh_leeway_ms,
            request_timeout: Duration::from_millis(request_timeout_ms),
        })
    }

    /// Seconds value used in user-facing timeout messages (v1 formatting).
    pub fn request_timeout_label(&self) -> String {
        crate::util::js_seconds(self.request_timeout.as_millis() as u64)
    }

    pub fn callback_timeout_label(&self) -> String {
        crate::util::js_seconds(self.callback_timeout.as_millis() as u64)
    }

    /// Issuer base path without a trailing slash ("" for a bare origin).
    pub fn issuer_base_path(&self) -> String {
        let path = self.issuer_url.path().trim_end_matches('/');
        path.to_string()
    }
}

fn normalize_issuer(value: &str) -> Result<String> {
    let url = Url::parse(value)
        .map_err(|_| AidError::new("AIDC_ISSUER must be a valid absolute URL."))?;

    let host = url.host_str().unwrap_or("");
    let local_host = matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1");
    if url.scheme() != "https" && !(url.scheme() == "http" && local_host) {
        return Err(AidError::new(
            "AIDC_ISSUER must use HTTPS unless it targets localhost.",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AidError::new(
            "AIDC_ISSUER must not include credentials, query parameters, or a fragment.",
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(AidError::new(
            "AIDC_ISSUER must not include credentials, query parameters, or a fragment.",
        ));
    }

    Ok(url.as_str().trim_end_matches('/').to_string())
}

fn parse_port(value: Option<&str>) -> Result<u16> {
    match value {
        None | Some("") => Ok(DEFAULT_CALLBACK_PORT),
        Some(raw) => {
            let parsed = raw.trim().parse::<i64>().ok();
            match parsed {
                Some(port) if (1..=65535).contains(&port) => Ok(port as u16),
                _ => Err(AidError::new(
                    "AIDC_PORT must be an integer between 1 and 65535.",
                )),
            }
        }
    }
}

fn parse_positive_int(value: Option<&str>, fallback: u64) -> Result<u64> {
    match value {
        None | Some("") => Ok(fallback),
        Some(raw) => match raw.trim().parse::<u64>() {
            Ok(parsed) if parsed > 0 && parsed <= i64::MAX as u64 => Ok(parsed),
            _ => Err(AidError::new(
                "Timeout and refresh-leeway environment values must be positive integers.",
            )),
        },
    }
}

fn normalize_scopes(value: Option<&str>) -> Result<String> {
    let scopes = value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_SCOPES);
    if scopes.is_empty() {
        return Err(AidError::new(
            "AIDC_SCOPES must contain a space-separated list of scopes.",
        ));
    }
    Ok(scopes.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(pairs: &[(&str, &str)]) -> Result<Config> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        Config::from_lookup(|key| {
            pairs
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        })
    }

    #[test]
    fn defaults_match_v1() {
        let config = config(&[]).expect("defaults");
        assert_eq!(config.issuer, "https://identity.ace-base.cc");
        assert_eq!(config.client_id, "aidc-cli");
        assert_eq!(config.callback_port, 7357);
        assert_eq!(config.redirect_uri, "http://127.0.0.1:7357/callback");
        assert_eq!(config.scopes, "openid profile email offline_access");
        assert_eq!(config.request_timeout, Duration::from_millis(15_000));
        assert_eq!(config.refresh_leeway_ms, 60_000);
    }

    #[test]
    fn issuer_requires_https_except_loopback() {
        assert!(config(&[("AIDC_ISSUER", "http://example.com")]).is_err());
        assert!(config(&[("AIDC_ISSUER", "http://127.0.0.1:8080")]).is_ok());
        assert!(config(&[("AIDC_ISSUER", "http://localhost:8080")]).is_ok());
        assert!(config(&[("AIDC_ISSUER", "not a url")]).is_err());
    }

    #[test]
    fn issuer_rejects_credentials_query_and_fragment() {
        assert!(config(&[("AIDC_ISSUER", "https://user:pw@example.com")]).is_err());
        assert!(config(&[("AIDC_ISSUER", "https://example.com?x=1")]).is_err());
        assert!(config(&[("AIDC_ISSUER", "https://example.com#frag")]).is_err());
    }

    #[test]
    fn issuer_trailing_slashes_are_stripped() {
        let config = config(&[("AIDC_ISSUER", "https://example.com/base///")]).expect("issuer");
        assert_eq!(config.issuer, "https://example.com/base");
        assert_eq!(config.issuer_base_path(), "/base");
    }

    #[test]
    fn port_must_be_in_range() {
        assert!(config(&[("AIDC_PORT", "0")]).is_err());
        assert!(config(&[("AIDC_PORT", "70000")]).is_err());
        assert!(config(&[("AIDC_PORT", "abc")]).is_err());
        assert_eq!(
            config(&[("AIDC_PORT", "9000")]).unwrap().callback_port,
            9000
        );
    }

    #[test]
    fn timeouts_must_be_positive_integers() {
        assert!(config(&[("AIDC_REQUEST_TIMEOUT_MS", "0")]).is_err());
        assert!(config(&[("AIDC_REQUEST_TIMEOUT_MS", "-5")]).is_err());
        assert!(config(&[("AIDC_REFRESH_LEEWAY_MS", "1.5")]).is_err());
        let config = config(&[("AIDC_REQUEST_TIMEOUT_MS", "2500")]).unwrap();
        assert_eq!(config.request_timeout_label(), "2.5");
    }
}
