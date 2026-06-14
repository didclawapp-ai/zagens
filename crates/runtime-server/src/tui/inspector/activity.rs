//! Harness / subagent / CRAFT activity log (right-rail Activity tab).

use ratatui::style::Color;
use ratatui::text::{Line, Span};

use super::super::display_format::truncate_display_width;
use super::super::theme::{self, TuiPanel};

const INSPECTOR: TuiPanel = TuiPanel::Inspector;

pub fn line_count(events: &[String]) -> usize {
    if events.is_empty() { 1 } else { events.len() }
}

fn event_style(text: &str) -> ratatui::style::Style {
    let lower = text.to_ascii_lowercase();
    if lower.contains("craft review") && (lower.contains("fail") || lower.contains("blocker")) {
        return theme::panel(INSPECTOR).item(false).fg(Color::Red);
    }
    if lower.contains("subagent spawned") {
        return theme::footer_chip(theme::footer_lht());
    }
    if lower.contains("subagent done") {
        return theme::panel(INSPECTOR).hint();
    }
    if lower.starts_with("harness:") || lower.starts_with("turn end:") {
        return theme::panel(INSPECTOR).item(false);
    }
    theme::panel(INSPECTOR).hint()
}

pub fn render_styled_panel(
    events: &[String],
    height: usize,
    scroll: usize,
    max_cols: usize,
) -> Vec<Line<'static>> {
    let max_cols = max_cols.max(8);
    let visible = height.max(4);
    let lines: Vec<Line<'static>> = if events.is_empty() {
        vec![Line::from(Span::styled(
            "(no harness activity)",
            theme::panel(INSPECTOR).hint(),
        ))]
    } else {
        events
            .iter()
            .map(|event| {
                let row = truncate_display_width(event, max_cols);
                Line::from(Span::styled(row, event_style(event)))
            })
            .collect()
    };
    let max_scroll = lines.len().saturating_sub(visible);
    let start = scroll.min(max_scroll);
    lines.into_iter().skip(start).take(visible).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_activity_placeholder() {
        assert_eq!(line_count(&[]), 1);
    }

    #[test]
    fn craft_fail_uses_warning_style() {
        let style = event_style("craft review: FAIL");
        assert_eq!(style.fg, Some(Color::Red));
    }
}
