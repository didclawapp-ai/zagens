//! Transcript line wrapping and internal engine status filtering.

use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

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
    // Redundant when tool rows are already visible in the transcript.
    if lowered.contains("executing tools sequentially")
        || (lowered.contains("tool call") && lowered.contains("running"))
        || lowered.contains("auto-loaded deferred tool")
        || lowered.contains("deferred tool")
    {
        return None;
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
    (since.elapsed().as_millis() as u64 / COMPOSER_CURSOR_BLINK_MS).is_multiple_of(2)
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
        format!("{spin} tool running | {focus_name}")
    } else {
        format!("{spin} tools running ({pending}) | {focus_name}")
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

const TRANSCRIPT_INDENT_4: &str = "    ";
const TRANSCRIPT_INDENT_5: &str = "     ";

/// Wrap transcript role lines with hanging indent so continuation rows stay aligned
/// and never exceed `max_cols` (avoids terminal soft-wrap / row overlap while streaming).
pub fn wrap_transcript_line(line: &str, max_cols: usize) -> Vec<String> {
    if let Some(body) = line.strip_prefix("AI> ") {
        return wrap_with_hanging("AI> ", TRANSCRIPT_INDENT_4, body, max_cols);
    }
    if let Some(body) = line.strip_prefix("you> ") {
        return wrap_with_hanging("you> ", TRANSCRIPT_INDENT_4, body, max_cols);
    }
    if let Some(body) = line.strip_prefix("THK> ") {
        return wrap_with_hanging("THK> ", TRANSCRIPT_INDENT_5, body, max_cols);
    }
    if let Some(body) = line.strip_prefix("tool ") {
        return wrap_with_hanging("tool ", TRANSCRIPT_INDENT_4, body, max_cols);
    }
    if let Some(body) = line.strip_prefix("-- ") {
        return wrap_with_hanging("-- ", TRANSCRIPT_INDENT_4, body, max_cols);
    }
    if let Some(body) = line.strip_prefix(TRANSCRIPT_INDENT_5) {
        return wrap_with_hanging(TRANSCRIPT_INDENT_5, TRANSCRIPT_INDENT_5, body, max_cols);
    }
    if let Some(body) = line.strip_prefix(TRANSCRIPT_INDENT_4) {
        return wrap_with_hanging(TRANSCRIPT_INDENT_4, TRANSCRIPT_INDENT_4, body, max_cols);
    }
    wrap_display_line(line, max_cols)
}

fn wrap_with_hanging(head: &str, cont: &str, body: &str, max_cols: usize) -> Vec<String> {
    let max_cols = max_cols.max(8);
    if body.is_empty() {
        return vec![head.to_string()];
    }
    if display_width(head) >= max_cols {
        return vec![truncate_display_width(&format!("{head}{body}"), max_cols)];
    }

    let mut out = Vec::new();
    let mut rest = body;
    let mut first = true;
    while !rest.is_empty() {
        let prefix = if first { head } else { cont };
        let budget = max_cols.saturating_sub(display_width(prefix));
        if budget == 0 {
            break;
        }
        let (segment, remaining) = take_wrapped_segment(rest, budget);
        if segment.is_empty() {
            break;
        }
        out.push(format!("{prefix}{segment}"));
        rest = remaining;
        first = false;
    }
    if out.is_empty() {
        out.push(head.to_string());
    }
    out
}

fn take_wrapped_segment(text: &str, budget: usize) -> (String, &str) {
    if budget == 0 {
        return (String::new(), text);
    }
    if display_width(text) <= budget {
        return (text.to_string(), "");
    }

    let start = text.len() - text.trim_start().len();
    let mut remaining = &text[start..];
    let mut out = String::new();

    if remaining.contains(' ') {
        while !remaining.is_empty() {
            remaining = remaining.trim_start();
            if remaining.is_empty() {
                break;
            }
            let word_end = remaining
                .find(char::is_whitespace)
                .unwrap_or(remaining.len());
            let word = &remaining[..word_end];
            let add = if out.is_empty() {
                display_width(word)
            } else {
                1 + display_width(word)
            };
            if display_width(&out) + add > budget {
                break;
            }
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(word);
            remaining = &remaining[word_end..];
        }
    }

    if out.is_empty() {
        return take_chars_width(remaining.trim_start(), budget);
    }

    let consumed = text.len() - remaining.len();
    (out, text[consumed..].trim_start())
}

fn take_chars_width(text: &str, budget: usize) -> (String, &str) {
    let mut out = String::new();
    let mut width = 0usize;
    for (byte_idx, ch) in text.char_indices() {
        let cw = char_width(ch);
        if width + cw > budget && !out.is_empty() {
            return (out, &text[byte_idx..]);
        }
        if cw > budget && out.is_empty() {
            out.push(ch);
            return (out, &text[byte_idx + ch.len_utf8()..]);
        }
        out.push(ch);
        width += cw;
    }
    (out, "")
}

pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Pad a line to `width` display columns so shorter redraws erase prior frame tails.
pub fn pad_line_display_width(line: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let w = display_width(line);
    if w > width {
        return truncate_display_width(line, width);
    }
    let mut out = String::with_capacity(line.len() + (width - w));
    out.push_str(line);
    out.push_str(&" ".repeat(width - w));
    out
}

/// Style used to paint row background before spans (full-width fill).
fn row_paint_style(line: &Line<'_>, blank_style: Style) -> Style {
    if line.spans.is_empty() {
        return blank_style;
    }
    if line.spans.len() == 1 {
        let style = line.spans[0].style;
        if style.bg.is_some() {
            return style;
        }
    }
    let first_bg = line.spans[0].style.bg;
    if first_bg.is_some() && line.spans.iter().all(|s| s.style.bg == first_bg) {
        return line.spans[0].style;
    }
    blank_style
}

/// Trailing pad style: extend explicit row backgrounds across the full pane width.
fn trailing_pad_style(line: &Line<'_>, pad_style: Style) -> Style {
    if line.spans.len() == 1 {
        let style = line.spans[0].style;
        if style.bg.is_some() {
            return style;
        }
    }
    pad_style
}

/// Pad a styled line to `width` display columns (append spaces with `pad_style`).
pub fn pad_styled_line(line: Line<'static>, width: usize, pad_style: Style) -> Line<'static> {
    if width == 0 {
        return line;
    }
    let plain: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    let w = display_width(&plain);
    if w > width {
        let truncated = truncate_display_width(&plain, width);
        let style = line
            .spans
            .iter()
            .find(|s| !s.content.is_empty())
            .map(|s| s.style)
            .unwrap_or(pad_style);
        return Line::from(Span::styled(truncated, style));
    }
    let row_pad = trailing_pad_style(&line, pad_style);
    if line.spans.len() == 1 {
        let span = &line.spans[0];
        let content = span.content.as_ref();
        if w < width {
            let pad = pad_line_display_width("", width.saturating_sub(w));
            return Line::from(vec![
                Span::styled(content.to_string(), span.style),
                Span::styled(pad, row_pad),
            ]);
        }
        return line;
    }
    if w < width {
        let pad = pad_line_display_width("", width.saturating_sub(w));
        let mut spans = line.spans;
        spans.push(Span::styled(pad, row_pad));
        return Line::from(spans);
    }
    line
}

/// Pad each line to `width` and extend with blank rows to `height` (clears ghost cells).
pub fn fill_styled_lines(
    lines: Vec<Line<'static>>,
    height: usize,
    width: usize,
    blank_style: Style,
) -> Vec<Line<'static>> {
    let height = height.max(1);
    let mut out: Vec<Line<'static>> = lines
        .into_iter()
        .take(height)
        .map(|line| pad_styled_line(line, width, blank_style))
        .collect();
    while out.len() < height {
        out.push(Line::from(Span::styled(
            pad_line_display_width("", width),
            blank_style,
        )));
    }
    out
}

/// Paint styled lines cell-by-cell (sidebar panes).
///
/// `Paragraph` can leave stale terminal cells on Windows when rows start with spaces;
/// writing every cell each frame prevents ghost `|` artifacts until the first keypress.
pub fn paint_styled_lines_in_area(
    buf: &mut Buffer,
    area: Rect,
    lines: &[Line<'static>],
    blank_style: Style,
) {
    let area = area.intersection(buf.area);
    if area.width == 0 || area.height == 0 {
        return;
    }
    let width = area.width as usize;
    let height = area.height as usize;
    let filled = fill_styled_lines(lines.to_vec(), height, width, blank_style);

    for (row, line) in filled.iter().enumerate().take(height) {
        let y = area.y + row as u16;
        let row_style = row_paint_style(line, blank_style);
        for col in 0..width {
            let cell = &mut buf[(area.x + col as u16, y)];
            cell.set_char(' ');
            cell.set_style(row_style);
        }
        let mut col = 0usize;
        'spans: for span in &line.spans {
            for ch in span.content.chars() {
                if col >= width {
                    break 'spans;
                }
                let ch_width = char_width(ch);
                if ch_width == 0 {
                    continue;
                }
                if col + ch_width > width {
                    break 'spans;
                }
                buf[(area.x + col as u16, y)]
                    .set_char(ch)
                    .set_style(span.style);
                col += ch_width;
            }
        }
    }
    for row in filled.len()..height {
        let y = area.y + row as u16;
        for col in 0..width {
            buf[(area.x + col as u16, y)]
                .set_char(' ')
                .set_style(blank_style);
        }
    }
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
        if cw == 0 {
            continue;
        }
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
    fn skips_redundant_tool_execution_status() {
        assert!(summarize_status_message("Executing tools sequentially...").is_none());
        assert!(
            summarize_status_message("Auto-loaded deferred tool 'web_search' after model request.")
                .is_none()
        );
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
        assert!(line.contains("| read_file"));
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
    fn pad_line_truncates_when_wider_than_area() {
        let line = "你好世界测试数据";
        let padded = pad_line_display_width(line, 8);
        assert!(display_width(&padded) <= 8);
    }

    #[test]
    fn pad_styled_line_truncates_multi_span_overflow() {
        let line = Line::from(vec![
            Span::raw("[x] "),
            Span::raw("run_fetch_url_with_a_very_long_label"),
        ]);
        let padded = pad_styled_line(line, 20, Style::default());
        let plain: String = padded.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(display_width(&plain) <= 20);
    }

    #[test]
    fn pad_styled_line_single_span_extends_row_background() {
        use ratatui::style::Color;
        let text_style = Style::default().bg(Color::Red);
        let pad_style = Style::default().bg(Color::Blue);
        let line = Line::from(Span::styled("hi", text_style));
        let padded = pad_styled_line(line, 6, pad_style);
        assert_eq!(padded.spans.len(), 2);
        assert_eq!(padded.spans[0].content.as_ref(), "hi");
        assert_eq!(padded.spans[0].style, text_style);
        assert_eq!(padded.spans[1].style, text_style);
        assert_eq!(display_width(padded.spans[1].content.as_ref()), 4);
    }

    #[test]
    fn pad_styled_line_single_span_without_bg_uses_pad_style() {
        use ratatui::style::Color;
        let text_style = Style::default().fg(Color::Red);
        let pad_style = Style::default().bg(Color::Blue);
        let line = Line::from(Span::styled("hi", text_style));
        let padded = pad_styled_line(line, 6, pad_style);
        assert_eq!(padded.spans[1].style, pad_style);
    }

    #[test]
    fn fill_styled_lines_pads_width_and_height() {
        let blank = Style::default();
        let lines = fill_styled_lines(vec![Line::from("hi")], 3, 10, blank);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans[0].content.as_ref(), "hi");
        assert_eq!(display_width(lines[0].spans[1].content.as_ref()), 8);
        assert_eq!(display_width(lines[1].spans[0].content.as_ref()), 10);
    }

    #[test]
    fn paint_styled_lines_in_area_writes_leading_spaces() {
        let area = Rect::new(0, 0, 8, 1);
        let mut buf = Buffer::empty(area);
        let style = Style::default().bg(ratatui::style::Color::Black);
        paint_styled_lines_in_area(
            &mut buf,
            area,
            &[Line::from(Span::styled("  file", style))],
            style,
        );
        assert_eq!(buf[(0, 0)].symbol(), " ");
        assert_eq!(buf[(1, 0)].symbol(), " ");
        assert_eq!(buf[(2, 0)].symbol(), "f");
    }

    #[test]
    fn wraps_cjk_assistant_with_hanging_indent() {
        let line = "AI> 加载审计上下文存在上一段会话上次审讯记录已经清楚了现在制定计划";
        let wrapped = wrap_transcript_line(line, 24);
        assert!(wrapped.len() >= 2);
        assert!(wrapped.iter().all(|l| display_width(l) <= 24));
        assert!(wrapped[0].starts_with("AI> "));
        for cont in wrapped.iter().skip(1) {
            assert!(cont.starts_with(TRANSCRIPT_INDENT_4));
        }
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
