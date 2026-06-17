//! Left rail session list (Phase 2).

use std::collections::HashMap;

use ratatui::text::{Line, Span};

use crate::localization::{Locale, MessageId, tr};
use crate::runtime_threads::ThreadRecord;

use super::display_format::{display_width, truncate_display_width};
use super::i18n::inspector_tab_label;
use super::layout::InspectorTab;
use super::theme::{self, TuiPanel};

const LEFT: TuiPanel = TuiPanel::Left;
/// Keep text inset from the pane's right edge (borderless layout has no divider buffer).
const LEFT_RAIL_TEXT_MARGIN: usize = 2;

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

/// Display label for a session row (matches `/v1/threads/summary` title logic).
pub fn resolve_session_label(
    thread: &ThreadRecord,
    latest_turn_summary: Option<&str>,
    locale: Locale,
) -> String {
    thread
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            latest_turn_summary
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| tr(locale, MessageId::TuiNewSession).to_string())
}

impl SessionList {
    pub fn from_threads(threads: Vec<ThreadRecord>, active_id: &str, locale: Locale) -> Self {
        Self::from_threads_with_summaries(threads, active_id, &HashMap::new(), locale)
    }

    pub fn from_threads_with_summaries(
        threads: Vec<ThreadRecord>,
        active_id: &str,
        turn_summaries: &HashMap<String, String>,
        locale: Locale,
    ) -> Self {
        let entries: Vec<SessionEntry> = threads
            .into_iter()
            .map(|t| {
                let summary = turn_summaries.get(&t.id).map(String::as_str);
                let label = resolve_session_label(&t, summary, locale);
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

    pub fn render_styled_lines(
        &self,
        height: usize,
        pane_inner_cols: usize,
        locale: Locale,
    ) -> Vec<Line<'static>> {
        let max_cols = clip_width(pane_inner_cols);
        let mut lines = Vec::new();
        lines.push(styled_clip(
            tr(locale, MessageId::TuiLeftRailSessions),
            max_cols,
            theme::panel(LEFT).heading(),
        ));
        if self.entries.is_empty() {
            lines.push(styled_clip(
                tr(locale, MessageId::TuiLeftRailNoSessions),
                max_cols,
                theme::panel(LEFT).hint(),
            ));
        } else {
            let visible = height.max(6);
            // Scroll so that `selected` is always visible, centered where possible.
            let start = if self.entries.len() <= visible {
                0
            } else {
                let max_start = self.entries.len() - visible;
                let ideal = self.selected.saturating_sub(visible / 2);
                ideal.min(max_start)
            };
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
            tr(locale, MessageId::TuiLeftRailInspector),
            max_cols,
            theme::panel(LEFT).heading(),
        ));
        lines.push(styled_clip(
            tr(locale, MessageId::TuiLeftRailNavHint),
            max_cols,
            theme::panel(LEFT).hint(),
        ));
        lines
    }

    pub fn inspector_tab_line(
        active: InspectorTab,
        pane_inner_cols: usize,
        locale: Locale,
    ) -> Line<'static> {
        let max_cols = clip_width(pane_inner_cols);
        let spans: Vec<Span> = InspectorTab::ALL
            .iter()
            .enumerate()
            .flat_map(|(i, tab)| {
                let is_active = *tab == active;
                let mark = if is_active { ">" } else { " " };
                let label = format!("{mark}{}{} ", i + 1, inspector_tab_label(locale, *tab));
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
        let visible = height.max(6);
        let start = if self.entries.len() <= visible {
            0
        } else {
            let max_start = self.entries.len() - visible;
            let ideal = self.selected.saturating_sub(visible / 2);
            ideal.min(max_start)
        };
        self.entries
            .iter()
            .enumerate()
            .skip(start)
            .take(visible)
            .map(|(i, entry)| {
                let mark = if i == self.selected { ">" } else { " " };
                format!(
                    "{mark} {}  {} ({})",
                    truncate_display_width(&entry.id, 12),
                    truncate_display_width(&entry.label, 20),
                    entry.updated_hint
                )
            })
            .collect()
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

/// Compact row: `>refactor harness… 06-13` — session name + date; thread id is omitted.
fn format_session_line(
    mark: &str,
    id: &str,
    label: &str,
    updated: &str,
    max_cols: usize,
) -> String {
    let suffix = format!(" {updated}");
    let suffix_w = display_width(&suffix);
    let mark_w = display_width(mark);

    // Legacy fallback when callers still pass the thread id as the label.
    if label == id {
        let bare_id = id.strip_prefix("thr_").unwrap_or(id);
        let id_budget = max_cols.saturating_sub(mark_w + suffix_w);
        let id_part = truncate_display_width(bare_id, id_budget);
        let row = format!("{mark}{id_part}{suffix}");
        return truncate_display_width(&row, max_cols);
    }

    let label_budget = max_cols.saturating_sub(mark_w + suffix_w);
    let label_part = truncate_display_width(label, label_budget);
    truncate_display_width(&format!("{mark}{label_part}{suffix}"), max_cols)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::localization::Locale;

    #[test]
    fn session_line_fits_narrow_left_rail() {
        // 28-col pane → 26 usable after margin.
        let line = format_session_line(
            ">",
            "thr_5ec4ef0c9abc",
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
        assert!(
            line.contains("refactor"),
            "session name should be visible: {line:?}"
        );
        assert!(
            !line.contains("5ec4ef0"),
            "thread id should be hidden when a name exists: {line:?}"
        );
    }

    #[test]
    fn session_line_legacy_id_label_shows_id_once() {
        let id = "thr_b3820950";
        let line = format_session_line(">", id, id, "06-12", clip_width(28));
        assert!(
            line.contains("b3820950"),
            "legacy id-only rows should still show the id: {line:?}"
        );
        assert!(
            display_width(&line) <= clip_width(28),
            "line too wide ({}) for {} cols: {line:?}",
            display_width(&line),
            clip_width(28),
        );
    }

    #[test]
    fn resolve_session_label_prefers_title_then_turn_summary() {
        use chrono::Utc;

        let now = Utc::now();
        let mut thread = ThreadRecord {
            schema_version: 1,
            id: "thr_test".to_string(),
            created_at: now,
            updated_at: now,
            model: "deepseek-chat".to_string(),
            workspace: std::path::PathBuf::from("."),
            mode: "agent".to_string(),
            allow_shell: false,
            trust_mode: false,
            auto_approve: false,
            latest_turn_id: None,
            latest_response_bookmark: None,
            archived: false,
            system_prompt: None,
            task_id: None,
            title: Some("  Custom title  ".to_string()),
            task_type: "code".to_string(),
            coherence_state: Default::default(),
            scratchpad_run_id: None,
            scratchpad_run_history: None,
            checklist_snapshot: None,
            plan_snapshot: None,
        };
        assert_eq!(
            resolve_session_label(&thread, Some("turn summary"), Locale::En),
            "Custom title"
        );
        thread.title = None;
        assert_eq!(
            resolve_session_label(&thread, Some("turn summary"), Locale::En),
            "turn summary"
        );
        assert_eq!(
            resolve_session_label(&thread, None, Locale::En),
            "New Session"
        );
    }

    #[test]
    fn clip_width_reserves_margin() {
        assert_eq!(clip_width(28), 26);
        assert_eq!(clip_width(8), 6);
    }

    #[test]
    fn inspector_tab_line_fits_width() {
        let line = SessionList::inspector_tab_line(InspectorTab::Files, 28, Locale::En);
        let plain: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            display_width(&plain) <= clip_width(28),
            "tabs overflow: {plain:?}"
        );
    }
}
