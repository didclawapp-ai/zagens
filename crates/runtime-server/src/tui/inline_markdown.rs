//! Lightweight inline Markdown parser → ratatui `Span` list.
//!
//! Supported inline patterns:
//!   - `` `code` ``  → contrasting code colour
//!   - `**bold**`    → bold modifier
//!   - `*italic*`    → italic modifier (gracefully ignored if terminal has no italic)
//!   - `__bold__`    → bold modifier (alternative syntax)
//!
//! All delimiters are ASCII single-byte, so byte-position arithmetic is safe
//! even when the surrounding text contains multi-byte UTF-8 (CJK, emoji, etc.).

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

/// Parse inline markdown in `text` and return styled spans.
///
/// `base` — style for plain text segments.
/// `code` — style for `` `inline code` `` segments.
pub fn inline_spans(text: &str, base: Style, code: Style) -> Vec<Span<'static>> {
    let bold = base.add_modifier(Modifier::BOLD);
    let italic = base.add_modifier(Modifier::ITALIC);

    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut seg_start = 0usize; // start of the current plain-text segment
    let mut i = 0usize;

    // Emit a plain-text span from `seg_start` to `end`.
    macro_rules! flush {
        ($end:expr) => {
            if $end > seg_start {
                spans.push(Span::styled(text[seg_start..$end].to_string(), base));
            }
        };
    }

    while i < len {
        let b = bytes[i];

        // ── `code` ──────────────────────────────────────────────────────────
        if b == b'`'
            && let Some(close) = find_byte(bytes, i + 1, b'`')
        {
            flush!(i);
            spans.push(Span::styled(text[i + 1..close].to_string(), code));
            i = close + 1;
            seg_start = i;
            continue;
        }

        // ── **bold** or __bold__ ─────────────────────────────────────────────
        if b == b'*'
            && i + 1 < len
            && bytes[i + 1] == b'*'
            && let Some(close) = find_str(text, i + 2, "**")
        {
            flush!(i);
            spans.push(Span::styled(text[i + 2..close].to_string(), bold));
            i = close + 2;
            seg_start = i;
            continue;
        }
        if b == b'_'
            && i + 1 < len
            && bytes[i + 1] == b'_'
            && let Some(close) = find_str(text, i + 2, "__")
        {
            flush!(i);
            spans.push(Span::styled(text[i + 2..close].to_string(), bold));
            i = close + 2;
            seg_start = i;
            continue;
        }

        // ── *italic* ─────────────────────────────────────────────────────────
        // Only trigger if the next char is not another `*` (to avoid eating `**`).
        if b == b'*'
            && i + 1 < len
            && bytes[i + 1] != b'*'
            && let Some(close) = find_byte(bytes, i + 1, b'*')
        {
            flush!(i);
            spans.push(Span::styled(text[i + 1..close].to_string(), italic));
            i = close + 1;
            seg_start = i;
            continue;
        }

        i += 1;
    }

    // Flush any remaining plain text.
    flush!(len);

    if spans.is_empty() {
        spans.push(Span::styled(text.to_string(), base));
    }
    spans
}

/// Find next occurrence of `marker` byte starting at `start`.
/// Returns `None` if not found or would produce an empty match (`start == result`).
fn find_byte(bytes: &[u8], start: usize, marker: u8) -> Option<usize> {
    bytes[start..]
        .iter()
        .position(|&b| b == marker)
        .map(|pos| start + pos)
}

/// Find next occurrence of `marker` string starting at `start` (byte offset).
/// Returns `None` if not found or the match would be immediately at `start` (empty body).
fn find_str(text: &str, start: usize, marker: &str) -> Option<usize> {
    text[start..]
        .find(marker)
        .map(|rel| start + rel)
        .filter(|&pos| pos > start) // require non-empty content between delimiters
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Style};

    fn plain() -> Style {
        Style::default()
    }
    fn code_style() -> Style {
        Style::default().fg(Color::Cyan)
    }
    fn content(spans: &[Span]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn plain_text_unchanged() {
        let spans = inline_spans("hello world", plain(), code_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "hello world");
    }

    #[test]
    fn backtick_code_span() {
        let spans = inline_spans("use `cargo build` here", plain(), code_style());
        let text = content(&spans);
        assert_eq!(text, "use cargo build here");
        let code_span = spans.iter().find(|s| s.content == "cargo build").unwrap();
        assert_eq!(code_span.style.fg, Some(Color::Cyan));
    }

    #[test]
    fn double_star_bold() {
        let spans = inline_spans("this is **very** important", plain(), code_style());
        let bold_span = spans.iter().find(|s| s.content == "very").unwrap();
        assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn single_star_italic() {
        let spans = inline_spans("read *carefully*", plain(), code_style());
        let it = spans.iter().find(|s| s.content == "carefully").unwrap();
        assert!(it.style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn mixed_inline() {
        let spans = inline_spans("**bold** and `code`", plain(), code_style());
        assert!(spans.iter().any(|s| s.content == "bold"));
        assert!(spans.iter().any(|s| s.content == "code"));
    }

    #[test]
    fn cjk_text_preserved() {
        let spans = inline_spans("中文 **加粗** 结束", plain(), code_style());
        let text = content(&spans);
        assert_eq!(text, "中文 加粗 结束");
        let bold_span = spans.iter().find(|s| s.content == "加粗").unwrap();
        assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
    }
}
