//! Pending follow-up preview above the composer (CodeWhale-style).

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::localization::{Locale, MessageId, tr};

use super::display_format::{display_width, truncate_display_width};
use super::theme;

const MAX_PREVIEW_ROWS: usize = 6;
const PREVIEW_LINE_LIMIT: usize = 3;

pub fn preview_line_count(messages: &[String], width: usize) -> usize {
    render_queued_preview(Locale::En, messages, width)
        .len()
        .min(MAX_PREVIEW_ROWS)
}

pub fn render_queued_preview(
    locale: Locale,
    messages: &[String],
    width: usize,
) -> Vec<Line<'static>> {
    if messages.is_empty() || width < 8 {
        return Vec::new();
    }

    let dim = theme::hint().add_modifier(Modifier::DIM);
    let mut out = Vec::new();

    out.push(Line::from(vec![
        Span::raw("• "),
        Span::styled(tr(locale, MessageId::TuiPendingInputsTitle), dim),
    ]));

    for (idx, body) in messages.iter().enumerate() {
        if out.len() >= MAX_PREVIEW_ROWS.saturating_sub(1) {
            out.push(Line::from(Span::styled("    …", dim)));
            break;
        }
        let n = idx + 1;
        let prefix = format!(" ↳ {} #{n}: ", tr(locale, MessageId::TuiPendingQueuedKind));
        push_wrapped(&mut out, body, width, dim, &prefix);
    }

    out.push(Line::from(Span::styled(
        tr(locale, MessageId::TuiPendingEditHint),
        dim,
    )));

    out
}

fn push_wrapped(out: &mut Vec<Line<'static>>, raw: &str, width: usize, style: Style, prefix: &str) {
    let indent = " ".repeat(display_width(prefix));
    let body_width = width.saturating_sub(display_width(prefix)).max(1);
    let mut rows = 0usize;
    for (i, paragraph) in raw.split('\n').enumerate() {
        for (j, segment) in wrap_simple(paragraph, body_width).into_iter().enumerate() {
            if rows >= PREVIEW_LINE_LIMIT {
                out.push(Line::from(Span::styled(format!("{indent}…"), style)));
                return;
            }
            let row = if i == 0 && j == 0 {
                format!("{prefix}{segment}")
            } else {
                format!("{indent}{segment}")
            };
            out.push(Line::from(Span::styled(
                truncate_display_width(&row, width),
                style,
            )));
            rows += 1;
        }
    }
}

fn wrap_simple(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    if display_width(text) <= width {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut current = String::new();
    let mut w = 0usize;
    for word in text.split_whitespace() {
        let ww = display_width(word);
        let add = if current.is_empty() { ww } else { 1 + ww };
        if w + add > width && !current.is_empty() {
            out.push(current);
            current = word.to_string();
            w = ww;
        } else if current.is_empty() {
            current = word.to_string();
            w = ww;
        } else {
            current.push(' ');
            current.push_str(word);
            w += add;
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    if out.is_empty() {
        out.push(truncate_display_width(text, width));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_queue_zero_height() {
        assert_eq!(preview_line_count(&[], 80), 0);
    }

    #[test]
    fn queued_messages_render_header_and_hint() {
        let lines = render_queued_preview(Locale::En, &["follow up please".to_string()], 60);
        assert!(lines.len() >= 3);
        let plain: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(plain.contains("Pending"));
        assert!(plain.contains("follow up"));
    }
}
