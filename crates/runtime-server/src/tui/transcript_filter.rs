//! Transcript display filtering: compact tool lines, skip harness noise, terminal-safe text.

use serde_json::Value;
use unicode_width::UnicodeWidthChar;

const SUMMARY_MAX: usize = 88;
const URL_MAX: usize = 64;

/// Remove ANSI escapes and glyphs that break terminal width (emoji, U+FFFD, zero-width).
///
/// Zero-width characters (width = 0 or None) are **dropped** rather than replaced with
/// a space. Replacing them with spaces inserts visible whitespace between letters when
/// the AI returns thinking text that contains embedded zero-width Unicode (e.g. U+200B
/// ZERO WIDTH SPACE, U+200C/D non-joiners), which causes English words to render as
/// isolated letters separated by gaps.
pub fn sanitize_terminal_text(text: &str) -> String {
    strip_ansi_escapes(text)
        .chars()
        .filter_map(sanitize_char)
        .collect()
}

fn sanitize_char(ch: char) -> Option<char> {
    if ch == '\u{FFFD}' {
        // Replacement character: keep as visible space placeholder.
        return Some(' ');
    }
    match UnicodeWidthChar::width(ch) {
        // Zero-width / invisible chars (e.g. U+200B, U+FEFF, combining marks that
        // have no width): remove entirely so they don't inject phantom spaces.
        None | Some(0) => None,
        // Pathologically wide characters (width > 2): substitute a single space
        // so they don't blow out column accounting.
        Some(w) if w > 2 => Some(' '),
        _ => Some(ch),
    }
}

/// ECMA-48 CSI final byte: `@` through `~` (0x40–0x7E).
fn is_csi_final_byte(ch: char) -> bool {
    ch.is_ascii() && (0x40..=0x7E).contains(&(ch as u8))
}

fn strip_ansi_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if is_csi_final_byte(c) {
                        break;
                    }
                }
            }
            continue;
        }
        if ch == '\r' {
            continue;
        }
        out.push(ch);
    }
    out
}

/// One-line summary when a tool call starts (replaces raw JSON in the transcript header).
pub fn format_tool_started_summary(name: &str, input: &Value) -> String {
    match name {
        "web_search" => field_quote(input, &["query"]),
        "fetch_url" | "fetch" => field_plain(input, &["url"], URL_MAX),
        "read_file" | "write_file" | "edit_file" | "apply_patch" => {
            field_plain(input, &["path", "file_path"], SUMMARY_MAX)
        }
        "bash" | "shell" | "run_terminal_cmd" => {
            field_plain(input, &["command", "cmd"], SUMMARY_MAX)
        }
        "list_dir" => field_plain(input, &["path", "directory"], SUMMARY_MAX),
        "grep" | "search" => {
            let pattern = field_plain(input, &["pattern", "query"], 40);
            let path = field_plain(input, &["path"], 32);
            if pattern.is_empty() {
                path
            } else if path.is_empty() {
                pattern
            } else {
                truncate_plain(&format!("{pattern} in {path}"), SUMMARY_MAX)
            }
        }
        _ => compact_value(input),
    }
}

/// One-line summary when a tool call completes (replaces raw JSON/HTML in the header).
pub fn format_tool_result_summary(name: &str, content: &str, success: bool) -> String {
    if !success {
        return truncate_plain(content, SUMMARY_MAX);
    }
    if content.trim().is_empty() {
        return "ok".to_string();
    }
    if let Ok(value) = serde_json::from_str::<Value>(content) {
        return summarize_json_result(name, &value, content);
    }
    match name {
        "web_search" => summarize_text_search(content),
        "fetch_url" | "fetch" => summarize_fetched_body(content),
        "read_file" => summarize_text_preview(content, "file"),
        "bash" | "shell" | "run_terminal_cmd" => summarize_text_preview(content, "output"),
        _ => summarize_generic_body(content),
    }
}

fn summarize_json_result(name: &str, value: &Value, raw: &str) -> String {
    if let Some(arr) = value.as_array() {
        return format!("{} results", arr.len());
    }
    if let Some(results) = value.get("results").and_then(Value::as_array) {
        let n = results.len();
        if let Some(query) = value
            .get("query")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            return truncate_plain(&format!("{n} results · «{query}»"), SUMMARY_MAX);
        }
        return format!("{n} results");
    }
    if let Some(url) = value.get("url").and_then(|v| v.as_str()) {
        return truncate_plain(&format!("{url} · {} bytes", raw.len()), SUMMARY_MAX);
    }
    match name {
        "web_search" => summarize_text_search(raw),
        "fetch_url" | "fetch" => summarize_fetched_body(raw),
        _ => compact_value(value),
    }
}

fn summarize_text_search(content: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(content)
        && let Some(results) = value.get("results").and_then(Value::as_array)
    {
        let n = results.len();
        if let Some(query) = value.get("query").and_then(|v| v.as_str()) {
            return truncate_plain(&format!("{n} results · «{query}»"), SUMMARY_MAX);
        }
        return format!("{n} results");
    }
    summarize_generic_body(content)
}

fn summarize_fetched_body(content: &str) -> String {
    let bytes = content.len();
    let lines = content.lines().count();
    let lower = content.to_ascii_lowercase();
    if lower.contains("<html") || lower.contains("<!doctype") {
        return format!("html · {bytes} bytes · {lines} lines");
    }
    if content.trim_start().starts_with('{') || content.trim_start().starts_with('[') {
        return format!("json · {bytes} bytes");
    }
    format!("text · {bytes} bytes · {lines} lines")
}

fn summarize_text_preview(content: &str, label: &str) -> String {
    let collapsed: String = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return "ok".to_string();
    }
    truncate_plain(&format!("{label}: {collapsed}"), SUMMARY_MAX)
}

fn summarize_generic_body(content: &str) -> String {
    let bytes = content.len();
    let lines = content.lines().count();
    if lines <= 1 {
        return truncate_plain(content, SUMMARY_MAX);
    }
    format!("{bytes} bytes · {lines} lines")
}

fn field_quote(value: &Value, keys: &[&str]) -> String {
    field_str(value, keys)
        .map(|s| truncate_plain(&format!("«{s}»"), SUMMARY_MAX))
        .unwrap_or_else(|| compact_value(value))
}

fn field_plain(value: &Value, keys: &[&str], max: usize) -> String {
    field_str(value, keys)
        .map(|s| truncate_plain(&s, max))
        .unwrap_or_else(|| compact_value(value))
}

fn field_str(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = value
            .get(*key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            return Some(s.to_string());
        }
    }
    None
}

fn compact_value(value: &Value) -> String {
    let collapsed = match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let collapsed: String = collapsed.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_plain(&collapsed, SUMMARY_MAX)
}

pub fn truncate_plain(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max).collect();
        format!("{cut}…")
    }
}

pub fn format_compact_count(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M chars", n as f64 / 1_000_000.0)
    } else if n >= 10_000 {
        format!("{:.0}k chars", n as f64 / 1_000.0)
    } else {
        format!("{n} chars")
    }
}

/// Harness labels that are internal-only and should not appear in the transcript.
pub fn should_skip_harness_label(label: &str) -> bool {
    matches!(
        label,
        "gate_skip" | "gate_pass" | "checklist_persist" | "context_snapshot" | "context_usage"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn web_search_started_summary() {
        let s = format_tool_started_summary("web_search", &json!({"query": "东莞 天气"}));
        assert!(s.contains("东莞"));
        assert!(!s.contains("{"));
    }

    #[test]
    fn fetch_result_summary() {
        let body = "<html><body>weather</body></html>";
        let s = format_tool_result_summary("fetch_url", body, true);
        assert!(s.contains("html"));
        assert!(s.contains("bytes"));
    }

    #[test]
    fn sanitize_strips_replacement_char() {
        let s = sanitize_terminal_text("晴\u{FFFD}天");
        assert!(!s.contains('\u{FFFD}'));
        // U+FFFD is replaced by a visible space placeholder.
        assert_eq!(s, "晴 天");
    }

    #[test]
    fn sanitize_drops_zero_width_chars_not_spaces() {
        // Zero-width space (U+200B) between English letters should be dropped,
        // NOT replaced with a visible space — otherwise "was" renders as "w a s".
        let input = "w\u{200B}a\u{200B}s";
        let s = sanitize_terminal_text(input);
        assert_eq!(
            s, "was",
            "zero-width chars must be removed, not replaced with spaces"
        );
    }

    #[test]
    fn sanitize_drops_zero_width_nonjoiner() {
        let input = "re\u{200C}think\u{200C}s";
        let s = sanitize_terminal_text(input);
        assert_eq!(s, "rethinks");
    }

    #[test]
    fn compact_count_formats_thousands() {
        assert_eq!(format_compact_count(434_948), "435k chars");
    }

    #[test]
    fn strip_ansi_csi_final_byte_tilde() {
        let input = "\x1b[12~hello";
        assert_eq!(strip_ansi_escapes(input), "hello");
    }

    #[test]
    fn strip_ansi_drops_lone_escape_byte() {
        assert_eq!(strip_ansi_escapes("\x1bPhello"), "Phello");
    }
}
