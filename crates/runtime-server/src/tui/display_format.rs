//! Transcript line wrapping and internal engine status filtering.

use std::time::Instant;

use super::transcript_filter::{format_compact_count, should_skip_harness_label};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Skip high-volume reconciliation signals (not user-facing).
pub fn should_skip_status_message(message: &str) -> bool {
    let msg = message.trim();
    if msg.is_empty() {
        return true;
    }
    if msg.starts_with("long_horizon.checklist_persist:")
        || msg.starts_with("long_horizon.context_snapshot:")
    {
        return true;
    }
    if msg.contains("[stream-probe]")
        || msg.contains("[thinking-probe]")
        || msg.contains("[lht-probe]")
    {
        return true;
    }
    false
}

/// Map harness `long_horizon.*` status events to a one-line Transcript summary.
pub fn summarize_status_message(message: &str) -> Option<String> {
    let msg = message.trim();
    if should_skip_status_message(msg) {
        return None;
    }
    if let Some(rest) = msg.strip_prefix("long_horizon.") {
        let label = rest.split(':').next().unwrap_or(rest).trim();
        if label.is_empty() || should_skip_harness_label(label) {
            return None;
        }
        return Some(format!("harness: {label}"));
    }
    let lowered = msg.to_ascii_lowercase();
    if lowered.contains("executing tools sequentially") {
        return Some("status: running tools".to_string());
    }
    if lowered.contains("tool call") && lowered.contains("running") {
        return Some("status: tool running".to_string());
    }
    if msg.len() > 100 {
        return Some(format!("status: {}…", truncate_chars(msg, 96)));
    }
    Some(format!("status: {msg}"))
}

const THINKING_SPINNER: [&str; 4] = ["|", "/", "-", "\\"];

pub fn thinking_spinner_frame(tick: u64) -> &'static str {
    THINKING_SPINNER[(tick as usize) % THINKING_SPINNER.len()]
}

const THINKING_SPINNER_MS: u64 = 120;
pub const COMPOSER_CURSOR_BLINK_MS: u64 = 530;

pub fn composer_cursor_blink_on(since: Instant) -> bool {
    (since.elapsed().as_millis() as u64 / COMPOSER_CURSOR_BLINK_MS) % 2 == 0
}

pub fn thinking_spinner_frame_at(since: Instant) -> &'static str {
    let elapsed = since.elapsed().as_millis() as u64;
    thinking_spinner_frame(elapsed / THINKING_SPINNER_MS)
}

pub fn thinking_status_line(char_count: usize, anim_since: Option<Instant>) -> String {
    let spin = anim_since
        .map(thinking_spinner_frame_at)
        .unwrap_or_else(|| thinking_spinner_frame(0));
    if char_count == 0 {
        format!("{spin} thinking...")
    } else {
        format!("{spin} thinking... ({})", format_compact_count(char_count))
    }
}

pub fn tool_chain_status_line(
    pending: usize,
    focus_name: &str,
    anim_since: Option<Instant>,
) -> String {
    let spin = anim_since
        .map(thinking_spinner_frame_at)
        .unwrap_or_else(|| thinking_spinner_frame(0));
    if pending <= 1 {
        format!("{spin} tool running — {focus_name}")
    } else {
        format!("{spin} tools running ({pending}) — {focus_name}")
    }
}

/// Wrap a single display line to fit pane width (prevents bleed into sidebars).
///
/// Uses terminal display width (CJK = 2 cols) so wrapped lines never trigger
/// the host terminal's implicit soft-wrap, which would overlap TUI rows.
pub fn wrap_display_line(line: &str, max_cols: usize) -> Vec<String> {
    let max_cols = max_cols.max(8);
    if display_width(line) <= max_cols {
        return vec![line.to_string()];
    }
    if line.contains(' ') {
        return wrap_by_words(line, max_cols);
    }
    chunk_chars(line, max_cols)
}

pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Pad a line to `width` display columns so shorter redraws erase prior frame tails.
pub fn pad_line_display_width(line: &str, width: usize) -> String {
    let w = display_width(line);
    if w >= width {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len() + (width - w));
    out.push_str(line);
    out.push_str(&" ".repeat(width - w));
    out
}

/// Truncate to fit display columns (CJK-safe); appends `…` when trimmed.
pub fn truncate_display_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if display_width(text) <= max_width {
        return text.to_string();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for ch in text.chars() {
        let cw = char_width(ch);
        if w + cw > max_width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

fn char_width(ch: char) -> usize {
    UnicodeWidthChar::width(ch).unwrap_or(0)
}

fn wrap_by_words(line: &str, max_cols: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    for word in line.split_whitespace() {
        let word_width = display_width(word);
        if word_width > max_cols {
            if !current.is_empty() {
                out.push(current);
                current = String::new();
                current_width = 0;
            }
            out.extend(chunk_chars(word, max_cols));
            continue;
        }
        if current.is_empty() {
            current = word.to_string();
            current_width = word_width;
        } else if current_width + 1 + word_width <= max_cols {
            current.push(' ');
            current.push_str(word);
            current_width += 1 + word_width;
        } else {
            out.push(current);
            current = word.to_string();
            current_width = word_width;
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn chunk_chars(text: &str, max_cols: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut width = 0usize;
    for ch in text.chars() {
        let ch_w = char_width(ch);
        if width + ch_w > max_cols && !buf.is_empty() {
            out.push(buf);
            buf = String::new();
            width = 0;
        }
        buf.push(ch);
        width += ch_w;
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_internal_harness_persist() {
        assert!(should_skip_status_message(
            "long_horizon.checklist_persist: {\"items\":[]}"
        ));
        assert!(should_skip_status_message(
            "long_horizon.context_snapshot: {\"tokens\":1}"
        ));
    }

    #[test]
    fn skips_internal_gate_skip() {
        assert!(summarize_status_message("long_horizon.gate_skip: {\"reason\":\"x\"}").is_none());
    }

    #[test]
    fn shortens_tool_execution_status() {
        let line = summarize_status_message("Executing tools sequentially...").expect("line");
        assert_eq!(line, "status: running tools");
    }

    #[test]
    fn wraps_long_unbroken_string() {
        let wrapped = wrap_display_line(&"a".repeat(40), 12);
        assert!(wrapped.len() >= 3);
        assert!(wrapped.iter().all(|l| display_width(l) <= 12));
    }

    #[test]
    fn thinking_spinner_cycles() {
        assert_eq!(thinking_spinner_frame(0), "|");
        assert_eq!(thinking_spinner_frame(1), "/");
        assert_eq!(thinking_spinner_frame(2), "-");
        assert_eq!(thinking_spinner_frame(3), "\\");
        assert_eq!(thinking_spinner_frame(4), "|");
    }

    #[test]
    fn composer_cursor_blink_toggles() {
        let since = Instant::now();
        assert!(composer_cursor_blink_on(since));
        let later = since - std::time::Duration::from_millis(COMPOSER_CURSOR_BLINK_MS);
        assert!(!composer_cursor_blink_on(later));
    }

    #[test]
    fn tool_chain_status_includes_spinner() {
        let since = Instant::now() - std::time::Duration::from_millis(THINKING_SPINNER_MS * 2);
        let line = tool_chain_status_line(2, "read_file", Some(since));
        assert!(line.starts_with("- tools running (2)"));
        assert!(line.contains("read_file"));
    }

    #[test]
    fn thinking_status_includes_spinner() {
        let since = Instant::now() - std::time::Duration::from_millis(THINKING_SPINNER_MS);
        let line = thinking_status_line(120, Some(since));
        assert!(line.starts_with("/ thinking"));
        assert!(line.contains("120 chars"));
    }

    #[test]
    fn pad_line_fills_to_display_width() {
        let padded = pad_line_display_width("hi", 10);
        assert_eq!(display_width(&padded), 10);
    }

    #[test]
    fn wraps_cjk_by_display_width() {
        // Eight Han chars = 16 terminal columns; wrap at 10 cols → two lines.
        let line = "你好世界测试一二";
        let wrapped = wrap_display_line(line, 10);
        assert_eq!(wrapped.len(), 2);
        assert!(wrapped.iter().all(|l| display_width(l) <= 10));
        assert_eq!(wrapped.join(""), line);
    }
}
