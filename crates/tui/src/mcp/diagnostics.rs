use reqwest;

/// Bytes of a non-2xx response body to surface in connection errors.
pub(super) const ERROR_BODY_PREVIEW_BYTES: usize = 200;

/// Mask a URL so any embedded credentials in the userinfo portion (e.g.
/// `https://user:secret@host`) are replaced with `***`. Failures fall back to
/// the original string so we don't lose context — we never want masking to
/// produce an empty error.
pub(super) fn mask_url_secrets(url: &str) -> String {
    if let Ok(parsed) = reqwest::Url::parse(url) {
        let mut clone = parsed.clone();
        if !parsed.username().is_empty() || parsed.password().is_some() {
            let _ = clone.set_username("***");
            let _ = clone.set_password(Some("***"));
        }
        return clone.to_string();
    }
    url.to_string()
}

/// Mask any obvious token-like substrings in a body excerpt before surfacing
/// it. Conservative: replaces `Bearer <token>` and `api_key=...` shapes.
pub(super) fn redact_body_preview(body: &str) -> String {
    let mut out = body.to_string();
    if let Some(idx) = out.to_lowercase().find("bearer ") {
        let tail_start = idx + "bearer ".len();
        if tail_start < out.len() {
            let end = out[tail_start..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == ',')
                .map_or(out.len(), |off| tail_start + off);
            out.replace_range(tail_start..end, "***");
        }
    }
    for needle in ["api_key=", "apikey=", "api-key=", "token="] {
        if let Some(idx) = out.to_lowercase().find(needle) {
            let tail_start = idx + needle.len();
            let end = out[tail_start..]
                .find(|c: char| c.is_whitespace() || c == '&' || c == '"' || c == ',')
                .map_or(out.len(), |off| tail_start + off);
            out.replace_range(tail_start..end, "***");
        }
    }
    out
}

/// Read up to `max_bytes` of a reqwest Response body and produce a single-line
/// excerpt suitable for an error message. Best-effort — if the body can't be
/// read, returns the literal string `<no body>`.
pub(super) async fn bounded_body_excerpt(response: reqwest::Response, max_bytes: usize) -> String {
    let body_text = response.text().await.unwrap_or_default();
    if body_text.is_empty() {
        return "<no body>".to_string();
    }
    let trimmed: String = body_text.chars().take(max_bytes).collect();
    let suffix = if body_text.len() > trimmed.len() {
        "…"
    } else {
        ""
    };
    let one_line = trimmed.replace(['\n', '\r'], " ");
    format!("{}{}", redact_body_preview(&one_line), suffix)
}
