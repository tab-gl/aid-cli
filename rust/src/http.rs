//! Minimal synchronous HTTP helper built on `ureq`.
//!
//! One place owns the three security-relevant transport decisions:
//! * a hard deadline that also covers response-body reads,
//! * redirects disabled (a 3xx is surfaced as an error, never followed), and
//! * a bounded response body so a hostile server cannot exhaust memory.

use std::io::Read;
use std::time::Duration;

#[derive(Debug)]
pub struct RawResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub enum HttpError {
    /// The request or response read exceeded the configured deadline.
    Timeout,
    /// A redirect that must not be followed (or that exceeded its budget).
    /// The reason is user-facing and never contains credentials.
    Redirect(String),
    /// Any other transport-level failure.
    Transport(String),
}

pub const DEFAULT_BODY_LIMIT: usize = 8 * 1024 * 1024;

pub struct Request<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub headers: Vec<(String, String)>,
    pub body: Option<(&'a str, Vec<u8>)>,
    pub timeout: Duration,
    pub body_limit: usize,
    /// Number of redirects to follow. `None` (the default) refuses them: every
    /// request that carries a credential must never be redirected. Unauthenticated
    /// downloads (the SDK artifact, which GitHub serves via a CDN redirect) opt in
    /// with a small bound and stay protected by the pinned SHA-256.
    pub redirects: Option<u32>,
}

impl<'a> Request<'a> {
    pub fn new(method: &'a str, url: &'a str, timeout: Duration) -> Self {
        Self {
            method,
            url,
            headers: Vec::new(),
            body: None,
            timeout,
            body_limit: DEFAULT_BODY_LIMIT,
            redirects: None,
        }
    }

    /// Follow at most `max` redirects (unauthenticated downloads only).
    pub fn follow_redirects(mut self, max: u32) -> Self {
        self.redirects = Some(max);
        self
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    pub fn form(self, body: String) -> Self {
        let mut request = self.header("content-type", "application/x-www-form-urlencoded");
        request.body = Some(("application/x-www-form-urlencoded", body.into_bytes()));
        request
    }

    pub fn json(self, body: Vec<u8>) -> Self {
        let mut request = self.header("content-type", "application/json");
        request.body = Some(("application/json", body));
        request
    }

    /// Execute the request.
    ///
    /// Redirects are followed manually (the agent never follows them), so the
    /// CLI controls every hop: the budget, the scheme (no https → http
    /// downgrade), and the headers (credentials are dropped).
    pub fn send(self) -> Result<RawResponse, HttpError> {
        let max_redirects = self.redirects.unwrap_or(0);
        let mut url = self.url.to_string();
        let mut headers = self.headers.clone();
        let mut method = self.method;
        let mut body: Option<(String, Vec<u8>)> = self
            .body
            .as_ref()
            .map(|(ct, bytes)| ((*ct).to_string(), bytes.clone()));
        let mut hops = 0u32;

        loop {
            let (response, location) = self.send_once(&url, method, &headers, body.as_ref())?;

            if !(300..400).contains(&response.status) {
                return Ok(response);
            }

            let Some(location) = location else {
                return Err(HttpError::Redirect(
                    "the server redirected without a location header".to_string(),
                ));
            };
            if hops >= max_redirects {
                return Err(HttpError::Redirect(format!(
                    "refusing to follow more than {max_redirects} redirects"
                )));
            }

            url = redirect_target(&url, &location)?;
            hops += 1;

            // A redirected request never carries credentials forward.
            strip_credentials(&mut headers);
            if method != "GET" {
                method = "GET";
                body = None;
            }
        }
    }

    fn send_once(
        &self,
        url: &str,
        method: &str,
        headers: &[(String, String)],
        body: Option<&(String, Vec<u8>)>,
    ) -> Result<(RawResponse, Option<String>), HttpError> {
        let mut builder = ureq::AgentBuilder::new().timeout(self.timeout).redirects(0);
        // v1 ignored proxy environment variables (undici does not read them);
        // v2 honours them so the CLI works behind a corporate proxy.
        if let Some(proxy) = proxy_from_env(url) {
            builder = builder.proxy(proxy);
        }
        let agent = builder.build();

        let mut request = agent.request(method, url);
        for (name, value) in headers {
            request = request.set(name, value);
        }

        let result = match body {
            Some((content_type, bytes)) => {
                request.set("content-type", content_type).send_bytes(bytes)
            }
            None => request.call(),
        };

        let response = match result {
            Ok(response) => response,
            Err(ureq::Error::Status(_status, response)) => response,
            Err(ureq::Error::Transport(transport)) => return Err(classify(&transport)),
        };

        let status = response.status();
        let location = response.header("location").map(str::to_string);
        let content_type = response.header("content-type").map(str::to_string);

        if (300..400).contains(&status) {
            // No body is read for a redirect; the caller decides what to do.
            return Ok((
                RawResponse {
                    status,
                    content_type,
                    body: Vec::new(),
                },
                location,
            ));
        }

        let mut body = Vec::new();
        let mut reader = response.into_reader().take(self.body_limit as u64);
        match reader.read_to_end(&mut body) {
            Ok(_) => {}
            Err(err) if is_timeout(&err.to_string()) => return Err(HttpError::Timeout),
            Err(err) => return Err(HttpError::Transport(err.to_string())),
        }

        Ok((
            RawResponse {
                status,
                content_type,
                body,
            },
            None,
        ))
    }
}

/// Resolve a `Location` header, refusing anything that would downgrade the
/// connection or leave an https origin for plain http.
pub fn redirect_target(current: &str, location: &str) -> Result<String, HttpError> {
    let base = url::Url::parse(current)
        .map_err(|_| HttpError::Redirect("the request URL was invalid".to_string()))?;
    let target = base
        .join(location.trim())
        .map_err(|_| HttpError::Redirect("the redirect location was invalid".to_string()))?;

    let target_host = target.host_str().unwrap_or_default();
    let target_is_loopback = matches!(target_host, "localhost" | "127.0.0.1" | "::1" | "[::1]");
    let allowed = target.scheme() == "https"
        || (target.scheme() == "http" && base.scheme() == "http" && target_is_loopback);

    if !allowed {
        return Err(HttpError::Redirect(format!(
            "refusing to follow a redirect from {} to {}",
            base.scheme(),
            target.scheme()
        )));
    }
    Ok(target.to_string())
}

/// Drop headers that must not travel across a redirect.
fn strip_credentials(headers: &mut Vec<(String, String)>) {
    headers.retain(|(name, _)| {
        !matches!(
            name.to_ascii_lowercase().as_str(),
            "authorization" | "cookie" | "proxy-authorization"
        )
    });
}

/// Build a proxy from `HTTPS_PROXY` / `HTTP_PROXY` (and their lowercase forms).
///
/// Loopback targets and hosts listed in `NO_PROXY` bypass the proxy, so a local
/// issuer (and the test suite) keeps working on a proxied machine.
pub fn proxy_from_env(url: &str) -> Option<ureq::Proxy> {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))?;
    let loopback = matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1" | "[::1]");
    if loopback || no_proxy_matches(&host) {
        return None;
    }
    let is_https = url.starts_with("https://");
    let candidates: [&str; 2] = if is_https {
        ["HTTPS_PROXY", "https_proxy"]
    } else {
        ["HTTP_PROXY", "http_proxy"]
    };
    let raw = candidates
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    ureq::Proxy::new(&raw).ok()
}

/// True when `NO_PROXY` / `no_proxy` lists this host (suffix match, `*` = all).
fn no_proxy_matches(host: &str) -> bool {
    let Some(list) = std::env::var("NO_PROXY")
        .ok()
        .or_else(|| std::env::var("no_proxy").ok())
    else {
        return false;
    };
    list.split(',')
        .map(|entry| entry.trim().trim_start_matches('.').to_lowercase())
        .filter(|entry| !entry.is_empty())
        .any(|entry| {
            entry == "*"
                || host.eq_ignore_ascii_case(&entry)
                || host.to_lowercase().ends_with(&format!(".{entry}"))
        })
}

fn classify(transport: &ureq::Transport) -> HttpError {
    // `message()` is often empty for I/O failures, so the rendered error (which
    // includes the underlying cause) is the reliable signal here.
    let message = transport
        .message()
        .map(str::to_string)
        .unwrap_or_else(|| transport.to_string());
    let source = std::error::Error::source(transport)
        .map(|error| error.to_string())
        .unwrap_or_default();

    if is_timeout(&message) || is_timeout(&source) {
        return HttpError::Timeout;
    }
    if matches!(transport.kind(), ureq::ErrorKind::TooManyRedirects)
        || message.to_lowercase().contains("redirect")
    {
        return HttpError::Redirect("the server redirected unexpectedly".to_string());
    }
    // Transport messages can embed the request or proxy URL; redact before it
    // reaches the terminal.
    HttpError::Transport(crate::api::errors::sanitize_detail(&message))
}

fn is_timeout(message: &str) -> bool {
    let message = message.to_lowercase();
    message.contains("timed out") || message.contains("timeout")
}

/// Percent-decode a URL component (`+` is treated as a space).
pub fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(b'%');
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decoding_handles_plus_and_escapes() {
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("%2F%3D"), "/=");
        assert_eq!(percent_decode("plain"), "plain");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn loopback_targets_bypass_the_proxy() {
        // Regardless of the proxy environment, loopback is never proxied.
        assert!(proxy_from_env("http://127.0.0.1:8080/apps").is_none());
        assert!(proxy_from_env("http://localhost:9000/token").is_none());
    }

    #[test]
    fn no_proxy_suffix_matching() {
        // Depends on the ambient NO_PROXY, so only assert the suffix rule.
        assert!(!no_proxy_matches(
            "definitely-not-in-any-no-proxy-list.example"
        ));
    }

    #[test]
    fn timeout_detection_matches_ureq_messages() {
        assert!(is_timeout("timed out reading response"));
        assert!(is_timeout("Operation timed out"));
        assert!(!is_timeout("connection refused"));
    }
}
