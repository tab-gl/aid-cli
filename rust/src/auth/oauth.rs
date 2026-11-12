//! OAuth 2.0 Authorization Code flow with PKCE (S256).
//!
//! Port of `src/auth/{login,callback}.js`: browser hand-off, loopback callback
//! server bound to 127.0.0.1, state validation, single-use callback, and the
//! token endpoint calls with the same user-facing error messages as v1.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use url::Url;

use crate::auth::token::Tokens;
use crate::config::Config;
use crate::error::{AidError, Result};
use crate::http::{self, HttpError, Request};

const CALLBACK_PATH: &str = "/callback";
const MAX_REQUEST_BYTES: usize = 8 * 1024;

/// Normalised token endpoint response.
#[derive(Debug, Clone)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub token_type: Option<String>,
    pub scope: Option<String>,
    pub expires_in: Option<f64>,
}

impl TokenResponse {
    fn from_json(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let access_token = object.get("access_token")?.as_str()?.trim().to_string();
        if access_token.is_empty() {
            return None;
        }
        let string_field = |key: &str| {
            object
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|value| !value.is_empty())
        };
        let expires_in = object.get("expires_in").and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
                .filter(|value| value.is_finite() && *value > 0.0)
        });

        Some(Self {
            access_token,
            refresh_token: string_field("refresh_token"),
            id_token: string_field("id_token"),
            token_type: string_field("token_type"),
            scope: string_field("scope"),
            expires_in,
        })
    }

    /// Expiry timestamp in epoch milliseconds (default lifetime 3600s).
    pub fn expires_at(&self) -> i64 {
        let seconds = self.expires_in.unwrap_or(3600.0);
        crate::util::now_ms() + (seconds * 1000.0).round() as i64
    }
}

/// Build the authorize URL for the PKCE flow.
pub fn authorize_url(config: &Config, state: &str, challenge: &str) -> Result<String> {
    let mut url = Url::parse(&format!("{}/auth", config.issuer))
        .map_err(|_| AidError::new("AIDC_ISSUER must be a valid absolute URL."))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", &config.redirect_uri)
        .append_pair("scope", &config.scopes)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

fn form_encode(pairs: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (index, (key, value)) in pairs.iter().enumerate() {
        if index > 0 {
            out.push('&');
        }
        out.push_str(&percent_encode(key));
        out.push('=');
        out.push_str(&percent_encode(value));
    }
    out
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Exchange an authorization code for tokens.
pub fn exchange_code(config: &Config, code: &str, verifier: &str) -> Result<TokenResponse> {
    let body = form_encode(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &config.redirect_uri),
        ("client_id", &config.client_id),
        ("code_verifier", verifier),
    ]);

    let response = Request::new(
        "POST",
        &format!("{}/token", config.issuer),
        config.request_timeout,
    )
    .form(body)
    .send()
    .map_err(|err| match err {
        HttpError::Timeout => AidError::new(format!(
            "Token exchange timed out after {}s.",
            config.request_timeout_label()
        )),
        HttpError::Redirect(_) | HttpError::Transport(_) => {
            AidError::new("Token endpoint request failed or was redirected unexpectedly.")
        }
    })?;

    if !(200..300).contains(&response.status) {
        let detail = http::RawResponse {
            status: response.status,
            content_type: response.content_type.clone(),
            body: response.body.clone(),
        };
        let detail = crate::api::errors::read_api_error(
            detail.status,
            detail.content_type.as_deref(),
            &detail.body,
        );
        return Err(AidError::new(format!(
            "Token exchange failed ({}): {}",
            response.status, detail
        )));
    }

    let value: Value = serde_json::from_slice(&response.body).map_err(|_| {
        AidError::new("Token endpoint returned invalid JSON. Run `aid login` again.")
    })?;
    TokenResponse::from_json(&value)
        .ok_or_else(|| AidError::new("Token endpoint returned no access token."))
}

/// Exchange a refresh token for a fresh access token.
pub fn refresh_tokens(config: &Config, refresh_token: &str) -> Result<TokenResponse> {
    let body = form_encode(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", &config.client_id),
    ]);

    let response = Request::new(
        "POST",
        &format!("{}/token", config.issuer),
        config.request_timeout,
    )
    .form(body)
    .send()
    .map_err(|err| match err {
        HttpError::Timeout => AidError::new(format!(
            "Token refresh timed out after {}s.",
            config.request_timeout_label()
        )),
        HttpError::Redirect(_) | HttpError::Transport(_) => {
            AidError::new("Token refresh request failed or was redirected unexpectedly.")
        }
    })?;

    if response.status == 400 || response.status == 401 {
        return Err(AidError::new(
            "Refresh failed because the session is no longer valid. Run `aid login` again.",
        ));
    }
    if !(200..300).contains(&response.status) {
        let detail = crate::api::errors::read_api_error(
            response.status,
            response.content_type.as_deref(),
            &response.body,
        );
        return Err(AidError::new(format!(
            "Token refresh failed ({}): {}",
            response.status, detail
        )));
    }

    let value: Value = serde_json::from_slice(&response.body).map_err(|_| {
        AidError::new("Token refresh returned invalid JSON. Run `aid login` again.")
    })?;
    TokenResponse::from_json(&value).ok_or_else(|| {
        AidError::new("Token refresh returned no access token. Run `aid login` again.")
    })
}

/// Merge a token response over the previous tokens, preserving fields the
/// server did not resend (matching v1's refresh behaviour).
pub fn merge_tokens(previous: Option<&Tokens>, response: &TokenResponse, issuer: &str) -> Tokens {
    let mut tokens = Tokens::new(response.access_token.clone());
    tokens.refresh_token = response
        .refresh_token
        .clone()
        .or_else(|| previous.and_then(|tokens| tokens.refresh_token.clone()));
    tokens.id_token = response
        .id_token
        .clone()
        .or_else(|| previous.and_then(|tokens| tokens.id_token.clone()));
    tokens.token_type = response
        .token_type
        .clone()
        .or_else(|| previous.and_then(|tokens| tokens.token_type.clone()))
        .or_else(|| Some("Bearer".to_string()));
    tokens.scope = response
        .scope
        .clone()
        .or_else(|| previous.and_then(|tokens| tokens.scope.clone()));
    tokens.expires_at = Some(response.expires_at());
    tokens.issuer = Some(issuer.trim_end_matches('/').to_string());
    if let Some(extra) = previous.map(|tokens| tokens.extra.clone()) {
        tokens.extra = extra;
    }
    tokens
}

/* ── Loopback callback server ───────────── */

enum CallbackOutcome {
    Code(String),
    /// A forged or unrelated request; keep waiting.
    Ignored,
    Failed(String),
}

/// A loopback listener for the OAuth redirect.
///
/// The listener is bound *before* the authorize URL is printed, so the browser
/// can never race the CLI to the callback port.
pub struct CallbackServer {
    listener: TcpListener,
    port: u16,
}

impl CallbackServer {
    /// Bind 127.0.0.1 on the configured callback port.
    pub fn bind(config: &Config) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", config.callback_port)).map_err(|err| {
            if err.kind() == std::io::ErrorKind::AddrInUse {
                AidError::new(format!(
                    "Port {} is already in use. Close the conflicting process or set AIDC_PORT.",
                    config.callback_port
                ))
            } else {
                AidError::new(format!(
                    "Could not start the local callback listener on port {}: {err}",
                    config.callback_port
                ))
            }
        })?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            port: config.callback_port,
        })
    }

    /// Port the listener is bound to.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Wait for the browser callback and return the authorization code.
    pub fn wait_for_code(&self, config: &Config, expected_state: &str) -> Result<String> {
        let deadline = Instant::now() + config.callback_timeout;
        loop {
            if Instant::now() >= deadline {
                return Err(AidError::new(format!(
                    "Timed out after {}s waiting for the browser.",
                    config.callback_timeout_label()
                )));
            }

            match self.listener.accept() {
                Ok((mut stream, _peer)) => {
                    stream.set_nonblocking(false)?;
                    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                    match handle_connection(&mut stream, expected_state) {
                        CallbackOutcome::Code(code) => return Ok(code),
                        CallbackOutcome::Ignored => continue,
                        CallbackOutcome::Failed(message) => return Err(AidError::new(message)),
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(err) => return Err(AidError::new(format!("Callback listener failed: {err}"))),
            }
        }
    }
}

/// Convenience wrapper used by callers that do not need to pre-bind.
pub fn wait_for_callback(config: &Config, expected_state: &str) -> Result<String> {
    let server = CallbackServer::bind(config)?;
    server.wait_for_code(config, expected_state)
}

fn handle_connection(stream: &mut TcpStream, expected_state: &str) -> CallbackOutcome {
    let request = match read_request_head(stream) {
        Some(request) => request,
        None => {
            respond(stream, 400, &failure_html("Invalid callback request"));
            return CallbackOutcome::Ignored;
        }
    };

    let Some(line) = request.lines().next() else {
        respond(stream, 400, &failure_html("Invalid callback request"));
        return CallbackOutcome::Ignored;
    };
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");

    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    };

    if method != "GET" || path != CALLBACK_PATH {
        respond(stream, 404, &failure_html("Not found"));
        return CallbackOutcome::Ignored;
    }

    let params = parse_query(query);

    // Forged or unrelated requests never consume the one-time callback.
    let state_matches = params
        .iter()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| crate::util::constant_time_eq(value, expected_state))
        .unwrap_or(false);
    if !state_matches {
        respond(stream, 400, &failure_html("Invalid state"));
        return CallbackOutcome::Ignored;
    }

    if params.iter().any(|(key, _)| key == "error") {
        respond(stream, 400, &failure_html("Sign-in failed"));
        return CallbackOutcome::Failed(
            "Authorization was rejected by the identity provider.".to_string(),
        );
    }

    let code = params
        .iter()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty());

    match code {
        Some(code) => {
            respond(stream, 200, &success_html());
            CallbackOutcome::Code(code)
        }
        None => {
            respond(stream, 400, &failure_html("Missing authorization code"));
            CallbackOutcome::Failed("No authorization code in callback.".to_string())
        }
    }
}

fn read_request_head(stream: &mut TcpStream) -> Option<String> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 512];
    loop {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if buffer.len() >= MAX_REQUEST_BYTES {
            break;
        }
    }
    if buffer.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&buffer).to_string())
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (http::percent_decode(key), http::percent_decode(value)),
            None => (http::percent_decode(pair), String::new()),
        })
        .collect()
}

fn respond(stream: &mut TcpStream, status: u16, html: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         content-type: text/html; charset=utf-8\r\n\
         cache-control: no-store\r\n\
         x-content-type-options: nosniff\r\n\
         content-length: {}\r\n\
         connection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn success_html() -> String {
    "<!doctype html><html><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>Signed in</title>\
<style>body{font-family:system-ui,sans-serif;max-width:420px;margin:20vh auto;padding:24px;color:#111;background:#f5f5f2;text-align:center}h1{font-size:24px;margin:0 0 12px}p{color:#666;line-height:1.6}</style>\
</head><body><h1>Signed in</h1><p>You can close this tab and return to your terminal.</p></body></html>"
        .to_string()
}

fn failure_html(message: &str) -> String {
    let escaped = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Sign-in failed</title></head>\
<body><h1>{escaped}</h1></body></html>"
    )
}

/* ── Browser hand-off ───────────────────── */

/// Open `url` in the user's browser, following the same platform rules as v1.
pub fn open_browser(url: &str) {
    let browser = std::env::var("BROWSER")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let (command, args): (String, Vec<String>) = if let Some(browser) = browser {
        (browser, vec![url.to_string()])
    } else if cfg!(target_os = "macos") {
        ("open".to_string(), vec![url.to_string()])
    } else if cfg!(windows) {
        ("explorer.exe".to_string(), vec![url.to_string()])
    } else if command_exists("termux-open-url") {
        ("termux-open-url".to_string(), vec![url.to_string()])
    } else if command_exists("xdg-open") {
        ("xdg-open".to_string(), vec![url.to_string()])
    } else {
        return;
    };

    let _ = Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn command_exists(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return true;
        }
        #[cfg(windows)]
        for extension in ["exe", "cmd", "bat"] {
            if dir.join(format!("{name}.{extension}")).is_file() {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_url_contains_pkce_parameters() {
        let config = Config::from_lookup(|_| None).unwrap();
        let url = authorize_url(&config, "state-value", "challenge-value").unwrap();
        assert!(url.starts_with("https://identity.ace-base.cc/auth?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("code_challenge=challenge-value"));
        assert!(url.contains("state=state-value"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A7357%2Fcallback"));
    }

    #[test]
    fn form_encoding_escapes_reserved_characters() {
        assert_eq!(
            form_encode(&[("a", "b c&d"), ("e", "/f")]),
            "a=b%20c%26d&e=%2Ff"
        );
    }

    #[test]
    fn token_response_requires_an_access_token() {
        assert!(TokenResponse::from_json(&serde_json::json!({})).is_none());
        assert!(TokenResponse::from_json(&serde_json::json!({"access_token": "  "})).is_none());
        let parsed =
            TokenResponse::from_json(&serde_json::json!({"access_token": "abc", "expires_in": 60}))
                .unwrap();
        assert_eq!(parsed.access_token, "abc");
        assert_eq!(parsed.expires_in, Some(60.0));
    }

    #[test]
    fn merged_tokens_keep_previous_refresh_token() {
        let mut previous = Tokens::new("old");
        previous.refresh_token = Some("refresh-1".to_string());
        previous.scope = Some("openid".to_string());
        let response = TokenResponse {
            access_token: "new".to_string(),
            refresh_token: None,
            id_token: None,
            token_type: None,
            scope: None,
            expires_in: Some(3600.0),
        };
        let merged = merge_tokens(Some(&previous), &response, "https://identity.ace-base.cc");
        assert_eq!(merged.access_token, "new");
        assert_eq!(merged.refresh_token.as_deref(), Some("refresh-1"));
        assert_eq!(merged.scope.as_deref(), Some("openid"));
        assert_eq!(merged.token_type.as_deref(), Some("Bearer"));
        assert!(merged.expires_at.unwrap() > crate::util::now_ms());
    }

    #[test]
    fn query_parsing_decodes_values() {
        let params = parse_query("state=a%2Bb&code=c%3Dd&empty=");
        assert_eq!(params[0], ("state".to_string(), "a+b".to_string()));
        assert_eq!(params[1], ("code".to_string(), "c=d".to_string()));
        assert_eq!(params[2], ("empty".to_string(), String::new()));
    }
}
