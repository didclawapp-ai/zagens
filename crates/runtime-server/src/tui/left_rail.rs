//! Left rail session list (Phase 2).

use ratatui::text::{Line, Span};

use crate::runtime_threads::ThreadRecord;

use super::display_format::{display_width, truncate_display_width};
use super::layout::InspectorTab;
use super::theme::{self, TuiPanel};

const LEFT: TuiPanel = TuiPanel::Left;
/// Keep text inset from the pane's right edge (borderless layout has no divider buffer).
const LEFT_RAIL_TEXT_MARGIN: usize = 2;
const SESSION_ID_MAX: usize = 7;

/// Usable text columns inside the left pane (caller passes `block.inner().width`).
pub fn clip_width(pane_inner_cols: usize) -> usize {
    pane_inner_cols.saturating_sub(LEFT_RAIL_TEXT_MARGIN).max(6)
}

#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub id: String,
    pub label: String,
    pub updated_hint: String,
}

#[derive(Debug, Clone, Default)]
pub struct SessionList {
    pub entries: Vec<SessionEntry>,
    pub selected: usize,
}

impl SessionList {
    pub fn from_threads(threads: Vec<ThreadRecord>, active_id: &str) -> Self {
        let entries: Vec<SessionEntry> = threads
            .into_iter()
            .map(|t| {
                let label = t
                    .title
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| t.id.clone());
                let updated_hint = t.updated_at.format("%m-%d").to_string();
                SessionEntry {
                    id: t.id,
                    label,
                    updated_hint,
                }
            })
            .collect();
        let selected = entries.iter().position(|e| e.id == active_id).unwrap_or(0);
        Self { entries, selected }
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.entries.get(self.selected).map(|e| e.id.as_str())
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    pub fn render_styled_lines(&self, height: usize, pane_inner_cols: usize) -> Vec<Line<'static>> {
        let max_cols = clip_width(pane_inner_cols);
        let mut lines = Vec::new();
        lines.push(styled_clip(
            "Sessions",
            max_cols,
            theme::panel(LEFT).heading(),
        ));
        if self.entries.is_empty() {
            lines.push(styled_clip(
                "(no sessions)",
                max_cols,
                theme::panel(LEFT).hint(),
            ));
        } else {
            let visible = height.max(6);
            let start = self.entries.len().saturating_sub(visible);
            for (i, entry) in self.entries.iter().enumerate().skip(start) {
                let selected = i == self.selected;
                let mark = if selected { ">" } else { " " };
                let text = format_session_line(
                    mark,
                    &entry.id,
                    &entry.label,
                    &entry.updated_hint,
                    max_cols,
                );
                lines.push(styled_clip(
                    &text,
                    max_cols,
                    theme::panel(LEFT).item(selected),
                ));
            }
        }

        lines.push(Line::from(Span::raw("")));
        lines.push(styled_clip(
            "Inspector",
            max_cols,
            theme::panel(LEFT).heading(),
        ));
        lines.push(styled_clip(
            "j/k Enter Ctrl+N",
            max_cols,
            theme::panel(LEFT).hint(),
        ));
        lines
    }

    pub fn inspector_tab_line(active: InspectorTab, pane_inner_cols: usize) -> Line<'static> {
        let max_cols = clip_width(pane_inner_cols);
        let spans: Vec<Span> = InspectorTab::ALL
            .iter()
            .enumerate()
            .flat_map(|(i, tab)| {
                let is_active = *tab == active;
                let mark = if is_active { ">" } else { " " };
                let label = format!("{mark}{}{} ", i + 1, tab.label());
                vec![Span::styled(label, theme::panel(LEFT).tab(is_active))]
            })
            .collect();
        let plain: String = spans.iter().map(|s| s.content.as_ref()).collect();
        if display_width(&plain) <= max_cols {
            return clip_spans_to_width(spans, max_cols);
        }
        styled_clip(&plain, max_cols, theme::panel(LEFT).tab(false))
    }

    pub fn render_lines(&self, height: usize) -> Vec<String> {
        if self.entries.is_empty() {
            return vec!["(no sessions)".to_string()];
        }
        let mut lines = Vec::new();
        for (i, entry) in self.entries.iter().enumerate() {
            let mark = if i == self.selected { ">" } else { " " };
            lines.push(format!(
                "{mark} {}  {} ({})",
                truncate_display_width(&entry.id, 12),
                truncate_display_width(&entry.label, 20),
                entry.updated_hint
            ));
        }
        if lines.len() > height.max(6) {
            let skip = lines.len() - height.max(6);
            lines.drain(0..skip);
        }
        lines
    }
}

fn styled_clip(text: &str, max_cols: usize, style: ratatui::style::Style) -> Line<'static> {
    Line::from(Span::styled(truncate_display_width(text, max_cols), style))
}

fn clip_spans_to_width(spans: Vec<Span<'static>>, max_cols: usize) -> Line<'static> {
    let plain: String = spans.iter().map(|s| s.content.as_ref()).collect();
    if display_width(&plain) <= max_cols {
        return Line::from(spans);
    }
    let style = spans
        .iter()
        .find(|s| !s.content.is_empty())
        .map(|s| s.style)
        .unwrap_or_else(|| theme::panel(LEFT).tab(false));
    styled_clip(&plain, max_cols, style)
}

/// Compact row: `>5ec4ef0 t… 06-13` — short id, date only, label truncated to fit.
fn format_session_line(
    mark: &str,
    id: &str,
    label: &str,
    updated: &str,
    max_cols: usize,
) -> String {
    let id_part = truncate_display_width(id.strip_prefix("mr_").unwrap_or(id), SESSION_ID_MAX);
    let suffix = format!(" {updated}");
    let suffix_w = display_width(&suffix);
    let prefix = format!("{mark}{id_part}");
    let prefix = if display_width(&prefix) + 1 + suffix_w <= max_cols {
        format!("{prefix} ")
    } else {
        prefix
    };
    let prefix_w = display_width(&prefix);
    let label_budget = max_cols.saturating_sub(prefix_w).saturating_sub(suffix_w);
    let label_part = if label_budget <= 1 {
        String::new()
    } else {
        truncate_display_width(label, label_budget)
    };
    truncate_display_width(&format!("{prefix}{label_part}{suffix}"), max_cols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_line_fits_narrow_left_rail() {
        // 28-col pane → 26 usable after margin.
        let line = format_session_line(
            ">",
            "mr_5ec4ef0c9abc",
            "refactor harness layout",
            "06-13",
            clip_width(28),
        );
        assert!(
            display_width(&line) <= clip_width(28),
            "line too wide ({}) for {} cols: {line:?}",
            display_width(&line),
            clip_width(28),
        );
    }

    #[test]
    fn clip_width_reserves_margin() {
        assert_eq!(clip_width(28), 26);
        assert_eq!(clip_width(8), 6);
    }

    #[test]
    fn inspector_tab_line_fits_width() {
        let line = SessionList::inspector_tab_line(InspectorTab::Files, 28);
        let plain: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            display_width(&plain) <= clip_width(28),
            "tabs overflow: {plain:?}"
        );
    }
}
