//! Sanitize HTTP error bodies before they reach UI, logs, or model context.

use regex::Regex;
use std::sync::OnceLock;

pub(crate) const LLM_ERROR_BODY_PREVIEW_BYTES: usize = 512;

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("HTML tag regex"))
}

fn bearer_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)bearer\s+[A-Za-z0-9._\-+/=]{8,}")
            .expect("bearer token regex")
    })
}

/// Strip HTML tags, control chars, and obvious secrets; cap length for display.
#[must_use]
pub fn sanitize_http_error_body(body: &str) -> String {
    let stripped = tag_re().replace_all(body, "");
    let visible: String = stripped
        .chars()
        .filter(|c| !c.is_control() || c.is_ascii_whitespace())
        .collect();
    let redacted = bearer_re()
        .replace_all(&visible, "Bearer [REDACTED]")
        .to_string();
    redact_kv_secrets(&redacted)
}

fn redact_kv_secrets(body: &str) -> String {
    let mut out = body.to_string();
    for needle in [
        "api_key=",
        "apikey=",
        "api-key=",
        "token=",
        "secret=",
        "password=",
    ] {
        let lower = out.to_lowercase();
        let Some(idx) = lower.find(needle) else {
            continue;
        };
        let tail_start = idx + needle.len();
        if tail_start >= out.len() {
            continue;
        }
        let end = out[tail_start..]
            .find(|c: char| c.is_whitespace() || c == '&' || c == '"' || c == ',' || c == '}')
            .map_or(out.len(), |off| tail_start + off);
        out.replace_range(tail_start..end, "[REDACTED]");
    }
    out
}

/// Sanitize and truncate to a bounded preview size (char-safe).
#[must_use]
pub fn truncate_http_error_body(body: &str) -> String {
    let stripped = sanitize_http_error_body(body);
    if stripped.len() <= LLM_ERROR_BODY_PREVIEW_BYTES {
        return stripped;
    }
    let mut end = LLM_ERROR_BODY_PREVIEW_BYTES;
    while end > 0 && !stripped.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &stripped[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_html_and_truncates_cloudflare_page() {
        let html = "<html><body><h1>502 Bad Gateway</h1><p>cloudflare ray id abc</p></body></html>";
        let out = truncate_http_error_body(html);
        assert!(!out.contains('<'));
        assert!(out.contains("502"));
    }

    #[test]
    fn redacts_bearer_and_api_key() {
        let body = r#"{"error":"invalid","Authorization":"Bearer sk-secret1234567890","api_key=leaked"}"#;
        let out = sanitize_http_error_body(body);
        assert!(!out.contains("sk-secret"));
        assert!(out.contains("[REDACTED]"));
    }
}
