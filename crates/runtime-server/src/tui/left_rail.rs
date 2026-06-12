//! Left rail session list (Phase 2).

use ratatui::text::{Line, Span};

use crate::runtime_threads::ThreadRecord;

use super::layout::InspectorTab;
use super::theme;

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
                let updated_hint = t.updated_at.format("%m-%d %H:%M").to_string();
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

    pub fn render_styled_lines(&self, height: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        lines.push(Line::from(Span::styled(
            "Sessions",
            theme::sidebar_heading(),
        )));
        if self.entries.is_empty() {
            lines.push(Line::from(Span::styled(
                "(no sessions)",
                theme::sidebar_hint(),
            )));
        } else {
            let visible = height.max(6);
            let start = self.entries.len().saturating_sub(visible);
            for (i, entry) in self.entries.iter().enumerate().skip(start) {
                let selected = i == self.selected;
                let mark = if selected { ">" } else { " " };
                let active = if entry.id.len() > 12 {
                    format!("{}…", &entry.id[..12])
                } else {
                    entry.id.clone()
                };
                let text = format!(
                    "{mark} {active}  {label} ({updated})",
                    label = truncate(&entry.label, 20),
                    updated = entry.updated_hint
                );
                lines.push(Line::from(Span::styled(
                    text,
                    theme::sidebar_item(selected),
                )));
            }
        }

        lines.push(Line::from(Span::raw("")));
        lines.push(Line::from(Span::styled(
            "Inspector",
            theme::sidebar_heading(),
        )));
        lines.push(Line::from(Span::styled(
            "j/k Enter Ctrl+N",
            theme::sidebar_hint(),
        )));
        lines
    }

    pub fn inspector_tab_line(active: InspectorTab) -> Line<'static> {
        let spans: Vec<Span> = InspectorTab::ALL
            .iter()
            .enumerate()
            .flat_map(|(i, tab)| {
                let is_active = *tab == active;
                let mark = if is_active { ">" } else { " " };
                let label = format!("{mark}{}{} ", i + 1, tab.label());
                vec![Span::styled(label, theme::sidebar_tab(is_active))]
            })
            .collect();
        Line::from(spans)
    }

    pub fn render_lines(&self, height: usize) -> Vec<String> {
        if self.entries.is_empty() {
            return vec!["(no sessions)".to_string()];
        }
        let mut lines = Vec::new();
        for (i, entry) in self.entries.iter().enumerate() {
            let mark = if i == self.selected { ">" } else { " " };
            let active = if entry.id.len() > 12 {
                format!("{}…", &entry.id[..12])
            } else {
                entry.id.clone()
            };
            lines.push(format!(
                "{mark} {active}  {label} ({updated})",
                label = truncate(&entry.label, 20),
                updated = entry.updated_hint
            ));
        }
        if lines.len() > height.max(6) {
            let skip = lines.len() - height.max(6);
            lines.drain(0..skip);
        }
        lines
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}
