//! API error rendering with secret redaction.
//!
//! Port of `src/api/errors.js`: bounded body reads, status labels, JSON detail
//! extraction, HTML suppression and redaction of tokens, credentials and URLs.

pub const MAX_ERROR_BYTES: usize = 4096;
pub const MAX_ERROR_CHARS: usize = 240;

fn status_label(status: u16) -> &'static str {
    match status {
        400 => "Bad request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not found",
        408 => "Request timeout",
        409 => "Conflict",
        413 => "Payload too large",
        422 => "Validation failed",
        429 => "Rate limited",
        _ => "HTTP error",
    }
}

/// Render an API error for display, e.g. `Not found: app 123 is gone`.
pub fn read_api_error(status: u16, content_type: Option<&str>, body: &[u8]) -> String {
    let label = status_label(status);
    let bounded = &body[..body.len().min(MAX_ERROR_BYTES)];
    let text = String::from_utf8_lossy(bounded).to_string();

    if text.trim().is_empty() {
        return label.to_string();
    }

    let mut detail = text.clone();
    let is_json = content_type
        .map(|value| value.to_lowercase().contains("json"))
        .unwrap_or(false);

    if is_json {
        detail = match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(serde_json::Value::Object(object)) => {
                ["message", "error_description", "detail", "error"]
                    .iter()
                    .find_map(|key| {
                        object
                            .get(*key)
                            .and_then(|value| value.as_str())
                            .filter(|value| !value.trim().is_empty())
                            .map(str::to_string)
                    })
                    .unwrap_or_default()
            }
            Ok(_) => String::new(),
            Err(_) => String::new(),
        };
    } else if looks_like_html(&text) {
        detail = String::new();
    }

    let detail = sanitize_detail(&detail);
    if detail.is_empty() {
        label.to_string()
    } else {
        format!("{label}: {detail}")
    }
}

fn looks_like_html(body: &str) -> bool {
    let trimmed = body.trim_start().to_lowercase();
    trimmed.starts_with("<!doctype html")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<body")
        || trimmed.starts_with("<head")
}

/// Remove secrets and URLs, collapse whitespace and bound the length.
pub fn sanitize_detail(value: &str) -> String {
    let without_urls = redact_urls(value);
    let without_bearer = redact_bearer(&without_urls);
    let without_assignments = redact_assignments(&without_bearer);

    let collapsed: String = without_assignments
        .chars()
        .map(|ch| {
            if ch == '\r' || ch == '\n' || ch == '\t' {
                ' '
            } else {
                ch
            }
        })
        .collect();
    let mut out = String::with_capacity(collapsed.len());
    let mut previous_space = false;
    for ch in collapsed.chars() {
        if ch == ' ' {
            if previous_space {
                continue;
            }
            previous_space = true;
        } else {
            previous_space = false;
        }
        out.push(ch);
    }
    out.trim().chars().take(MAX_ERROR_CHARS).collect()
}

fn redact_urls(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    loop {
        let lower = rest.to_lowercase();
        let index = match (lower.find("http://"), lower.find("https://")) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let Some(index) = index else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..index]);
        out.push_str("[URL]");
        let tail = &rest[index..];
        let end = tail
            .char_indices()
            .find(|(position, ch)| {
                *position > 0 && (ch.is_whitespace() || matches!(ch, '"' | '\'' | '<' | '>' | ','))
            })
            .map(|(position, _)| position)
            .unwrap_or(tail.len());
        rest = &tail[end..];
    }
}

fn redact_bearer(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut expecting_value = false;
    for (index, part) in value.split(' ').enumerate() {
        if index > 0 {
            out.push(' ');
        }
        if expecting_value {
            out.push_str("[REDACTED]");
            expecting_value = false;
            continue;
        }
        if part.eq_ignore_ascii_case("bearer") {
            expecting_value = true;
        }
        out.push_str(part);
    }
    out
}

const SENSITIVE_KEYS: [&str; 7] = [
    "access_token",
    "refresh_token",
    "id_token",
    "client_secret",
    "authorization",
    "api_key",
    "password",
];

fn redact_assignments(value: &str) -> String {
    let mut out = value.to_string();
    for key in SENSITIVE_KEYS {
        out = redact_key(&out, key);
    }
    // `api-key` and `apikey` spellings.
    out = redact_key(&out, "api-key");
    out = redact_key(&out, "apikey");
    out
}

fn redact_key(value: &str, key: &str) -> String {
    let lower = value.to_lowercase();
    let mut out = String::with_capacity(value.len());
    let mut cursor = 0;

    while let Some(found) = lower[cursor..].find(key) {
        let start = cursor + found;
        // Only redact when the key stands alone (word boundary on both sides).
        let before_ok = start == 0 || !lower.as_bytes()[start - 1].is_ascii_alphanumeric();
        let after = start + key.len();
        let after_ok = after >= lower.len() || !lower.as_bytes()[after].is_ascii_alphanumeric();
        if !before_ok || !after_ok {
            out.push_str(&value[cursor..after]);
            cursor = after;
            continue;
        }

        out.push_str(&value[cursor..after]);
        cursor = after;

        // Optional quote, optional whitespace, separator, optional quote.
        let mut probe = cursor;
        let bytes = value.as_bytes();
        if probe < bytes.len() && (bytes[probe] == b'"' || bytes[probe] == b'\'') {
            out.push(bytes[probe] as char);
            probe += 1;
        }
        let space_start = probe;
        while probe < bytes.len() && bytes[probe] == b' ' {
            probe += 1;
        }
        if probe < bytes.len() && (bytes[probe] == b':' || bytes[probe] == b'=') {
            out.push_str(&value[space_start..probe + 1]);
            probe += 1;
            while probe < bytes.len() && bytes[probe] == b' ' {
                out.push(' ');
                probe += 1;
            }
            if probe < bytes.len() && (bytes[probe] == b'"' || bytes[probe] == b'\'') {
                out.push(bytes[probe] as char);
                probe += 1;
            }
            let value_start = probe;
            while probe < bytes.len()
                && !bytes[probe].is_ascii_whitespace()
                && !matches!(bytes[probe], b'"' | b'\'' | b',' | b'}')
            {
                probe += 1;
            }
            if probe > value_start {
                out.push_str("[REDACTED]");
            }
            cursor = probe;
        } else {
            cursor = probe;
        }
    }

    out.push_str(&value[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_labels_match_v1() {
        assert_eq!(read_api_error(404, None, b""), "Not found");
        assert_eq!(read_api_error(500, None, b"   "), "HTTP error");
        assert_eq!(
            read_api_error(429, None, b"slow down"),
            "Rate limited: slow down"
        );
    }

    #[test]
    fn json_detail_prefers_message() {
        let body = br#"{"message":"app not found","error":"not_found"}"#;
        assert_eq!(
            read_api_error(404, Some("application/json"), body),
            "Not found: app not found"
        );
    }

    #[test]
    fn json_without_detail_falls_back_to_label() {
        assert_eq!(
            read_api_error(400, Some("application/json"), b"{\"unexpected\":true}"),
            "Bad request"
        );
        assert_eq!(
            read_api_error(400, Some("application/json"), b"not json"),
            "Bad request"
        );
    }

    #[test]
    fn html_bodies_are_suppressed() {
        let body = b"<!DOCTYPE html><html><body>Gateway error</body></html>";
        assert_eq!(read_api_error(502, Some("text/html"), body), "HTTP error");
    }

    #[test]
    fn secrets_are_redacted() {
        let detail = sanitize_detail("invalid access_token=abc123 for user");
        assert!(!detail.contains("abc123"));
        assert!(detail.contains("[REDACTED]"));

        let detail = sanitize_detail("bad Authorization: Bearer super-secret-value");
        assert!(!detail.contains("super-secret-value"));
    }

    #[test]
    fn urls_are_replaced() {
        let detail = sanitize_detail("see https://internal.example/token?x=1 for details");
        assert_eq!(detail, "see [URL] for details");
    }

    #[test]
    fn whitespace_is_collapsed_and_truncated() {
        let detail = sanitize_detail("a\n\n\tb    c");
        assert_eq!(detail, "a b c");

        let long = sanitize_detail(&"x".repeat(500));
        assert_eq!(long.chars().count(), MAX_ERROR_CHARS);
    }

    #[test]
    fn oversize_bodies_are_bounded() {
        let body = vec![b'a'; MAX_ERROR_BYTES * 2];
        let rendered = read_api_error(400, None, &body);
        assert!(rendered.len() <= MAX_ERROR_CHARS + "Bad request: ".len());
    }
}
