//! Authenticated API client.
//!
//! Port of `src/api/client.js`: resolve the URL, attach the (refreshed) bearer
//! token, never follow redirects, map transport failures to the same messages,
//! and drop the stored session when the server rejects the access token.

use serde_json::Value;

use crate::api::errors::read_api_error;
use crate::api::url::resolve_api_url;
use crate::auth::{refresh, token};
use crate::config::Config;
use crate::error::{AidError, Result};
use crate::http::{HttpError, RawResponse, Request};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Patch,
    Delete,
}

impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Debug)]
pub struct ApiResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

impl ApiResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn error_detail(&self) -> String {
        read_api_error(self.status, self.content_type.as_deref(), &self.body)
    }
}

impl From<RawResponse> for ApiResponse {
    fn from(raw: RawResponse) -> Self {
        Self {
            status: raw.status,
            content_type: raw.content_type,
            body: raw.body,
        }
    }
}

/// Perform an authenticated request and return the raw response.
pub fn api_fetch(
    config: &Config,
    path: &str,
    method: Method,
    json_body: Option<&Value>,
) -> Result<ApiResponse> {
    let url = resolve_api_url(config, path)?;
    let access_token = refresh::get_valid_access_token(config)?;

    let mut request = Request::new(method.as_str(), &url, config.request_timeout)
        .header("authorization", format!("Bearer {access_token}"))
        .header("accept", "application/json");
    if let Some(body) = json_body {
        let encoded = serde_json::to_vec(body)?;
        request = request.json(encoded);
    }

    let response = match request.send() {
        Ok(response) => ApiResponse::from(response),
        Err(HttpError::Timeout) => {
            return Err(AidError::new(format!(
                "Request timed out after {}s.",
                config.request_timeout_label()
            )))
        }
        Err(HttpError::Redirect(_)) => {
            return Err(AidError::new(
                "API request was redirected unexpectedly; refusing to forward credentials.",
            ))
        }
        Err(HttpError::Transport(_)) => {
            return Err(AidError::new(
                "Network request failed. Check your connection and try again.",
            ))
        }
    };

    if response.status == 401 {
        token::clear_if_access_token(&access_token).ok();
        return Err(AidError::new(
            "Server rejected the access token. Run `aid login` again.",
        ));
    }

    Ok(response)
}

/// Perform an authenticated request and decode a JSON response.
pub fn api_json(
    config: &Config,
    path: &str,
    method: Method,
    json_body: Option<&Value>,
) -> Result<Value> {
    let response = api_fetch(config, path, method, json_body)?;
    if !response.is_success() {
        return Err(AidError::new(format!(
            "Request failed ({}): {}",
            response.status,
            response.error_detail()
        )));
    }

    serde_json::from_slice(&response.body).map_err(|_| {
        AidError::new(format!(
            "Request succeeded but returned invalid JSON (HTTP {}).",
            response.status
        ))
    })
}

/// Convenience wrapper for `GET` requests returning JSON.
pub fn api_get_json(config: &Config, path: &str) -> Result<Value> {
    api_json(config, path, Method::Get, None)
}

/// Percent-encode a path segment (matches `encodeURIComponent` for identifiers).
pub fn encode_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        let ch = byte as char;
        if ch.is_ascii_alphanumeric()
            || matches!(ch, '-' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')')
        {
            out.push(ch);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_encoding_matches_encode_uri_component() {
        assert_eq!(encode_segment("app-1"), "app-1");
        assert_eq!(encode_segment("a/b c"), "a%2Fb%20c");
        assert_eq!(encode_segment("a?b=c"), "a%3Fb%3Dc");
    }
}
