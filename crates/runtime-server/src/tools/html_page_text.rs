//! HTML → readable plain text (block-aware line breaks).
//!
//! Shared by `fetch_url` and kept separate from `web_run` so both tools can
//! evolve without circular imports.

use regex::Regex;
use std::sync::OnceLock;

static SCRIPT_RE: OnceLock<Regex> = OnceLock::new();
static STYLE_RE: OnceLock<Regex> = OnceLock::new();
static TAG_RE: OnceLock<Regex> = OnceLock::new();
static BLOCK_RE: OnceLock<Regex> = OnceLock::new();
static TITLE_RE: OnceLock<Regex> = OnceLock::new();

fn script_re() -> &'static Regex {
    SCRIPT_RE.get_or_init(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("script re"))
}
fn style_re() -> &'static Regex {
    STYLE_RE.get_or_init(|| Regex::new(r"(?is)<style[^>]*>.*?</style>").expect("style re"))
}
fn tag_re() -> &'static Regex {
    TAG_RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("tag re"))
}
fn block_re() -> &'static Regex {
    BLOCK_RE.get_or_init(|| {
        Regex::new(r"(?is)</?(p|div|li|ul|ol|br|h[1-6]|tr|td|th|table|section|article)[^>]*>")
            .expect("block re")
    })
}
fn title_re() -> &'static Regex {
    TITLE_RE.get_or_init(|| Regex::new(r"(?is)<title[^>]*>(.*?)</title>").expect("title re"))
}

/// Convert HTML to readable plain text with preserved line breaks.
pub(crate) fn html_to_readable_text(html: &str) -> String {
    let title = title_re()
        .captures(html)
        .and_then(|cap| cap.get(1))
        .map(|m| normalize_whitespace(&decode_html_entities(m.as_str())))
        .filter(|t| !t.is_empty());

    let without_scripts = script_re().replace_all(html, "").to_string();
    let without_styles = style_re().replace_all(&without_scripts, "").to_string();
    let with_breaks = block_re().replace_all(&without_styles, "\n").to_string();
    let without_tags = tag_re().replace_all(&with_breaks, "").to_string();
    let decoded = decode_html_entities(&without_tags);

    let mut lines = Vec::new();
    for line in decoded.lines() {
        let trimmed = normalize_whitespace(line);
        if !trimmed.is_empty() {
            lines.push(trimmed);
        }
    }

    let body = lines.join("\n");
    match title {
        Some(t) if !body.is_empty() => format!("{t}\n\n{body}"),
        Some(t) => t,
        None => body,
    }
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_html_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_block_structure_and_title() {
        let html = r#"
            <html><head><title>Page Title</title></head>
            <body>
              <h1>Hello &amp; welcome</h1>
              <p>First paragraph.</p>
              <p>Second paragraph.</p>
              <script>alert("nope");</script>
            </body></html>
        "#;
        let text = html_to_readable_text(html);
        assert!(text.starts_with("Page Title"));
        assert!(text.contains("Hello & welcome"));
        assert!(text.contains("First paragraph."));
        assert!(text.contains("Second paragraph."));
        assert!(text.contains('\n'));
        assert!(!text.contains("alert"));
    }
}
