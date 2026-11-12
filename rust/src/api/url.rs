//! API URL resolution.
//!
//! Port of `src/api/url.js`. Every request path is resolved against the
//! configured issuer and then checked for containment, so a path (or a server
//! response) cannot redirect credentials to another origin.

use crate::config::Config;
use crate::error::{AidError, Result};

const OUTSIDE: &str = "Refusing API request outside the configured Ace ID issuer.";

/// Resolve `request_path` against the issuer origin and base path.
pub fn resolve_api_url(config: &Config, request_path: &str) -> Result<String> {
    if request_path.trim().is_empty() {
        return Err(AidError::new("API path must be a non-empty string."));
    }
    let raw_path = request_path.trim();

    if looks_absolute(raw_path) || raw_path.starts_with("//") {
        return Err(AidError::new(OUTSIDE));
    }

    let base_path = config.issuer_base_path();
    let base_dir = if base_path.is_empty() {
        "/".to_string()
    } else {
        format!("{base_path}/")
    };

    let mut base = config.issuer_url.clone();
    base.set_path(&base_dir);
    base.set_query(None);
    base.set_fragment(None);

    let relative = raw_path.trim_start_matches('/');
    let url = base.join(relative).map_err(|_| AidError::new(OUTSIDE))?;

    // `Url::join` normalises dot segments, so containment is checked after
    // resolution: traversal cannot escape the issuer base path.
    if url.origin() != config.issuer_url.origin() || !url.path().starts_with(&base_dir) {
        return Err(AidError::new(OUTSIDE));
    }

    Ok(url.to_string())
}

fn looks_absolute(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    for ch in chars {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '-' | '.' => continue,
            ':' => return true,
            _ => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(issuer: &str) -> Config {
        Config::from_lookup(|key| (key == "AIDC_ISSUER").then(|| issuer.to_string())).unwrap()
    }

    #[test]
    fn relative_paths_resolve_against_the_issuer() {
        let config = config("https://identity.ace-base.cc");
        assert_eq!(
            resolve_api_url(&config, "/apps").unwrap(),
            "https://identity.ace-base.cc/apps"
        );
        assert_eq!(
            resolve_api_url(&config, "apps/abc").unwrap(),
            "https://identity.ace-base.cc/apps/abc"
        );
    }

    #[test]
    fn base_path_is_preserved() {
        let config = config("https://example.com/identity");
        assert_eq!(
            resolve_api_url(&config, "/apps").unwrap(),
            "https://example.com/identity/apps"
        );
    }

    #[test]
    fn absolute_urls_are_rejected() {
        let config = config("https://identity.ace-base.cc");
        assert!(resolve_api_url(&config, "https://evil.example/apps").is_err());
        assert!(resolve_api_url(&config, "//evil.example/apps").is_err());
        assert!(resolve_api_url(&config, "javascript:alert(1)").is_err());
    }

    #[test]
    fn traversal_outside_the_issuer_is_rejected() {
        let config = config("https://example.com/identity");
        assert!(resolve_api_url(&config, "../../apps").is_err());
        assert!(resolve_api_url(&config, "%2e%2e/%2e%2e/apps").is_err());
        assert!(resolve_api_url(&config, "apps/../../..").is_err());
    }

    #[test]
    fn empty_paths_are_rejected() {
        let config = config("https://identity.ace-base.cc");
        assert!(resolve_api_url(&config, "  ").is_err());
    }
}
