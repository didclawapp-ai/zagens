//! Sub-agent list from cached engine events.

use ratatui::text::{Line, Span};

use super::super::theme::{self, TuiPanel};

const INSPECTOR: TuiPanel = TuiPanel::Inspector;

#[derive(Debug, Clone, Default)]
pub struct AgentEntry {
    pub id: String,
    pub status: String,
}

pub fn line_count(agents: &[AgentEntry]) -> usize {
    if agents.is_empty() { 1 } else { agents.len() }
}

pub fn render_styled_panel(
    agents: &[AgentEntry],
    height: usize,
    scroll: usize,
    cursor: usize,
    max_cols: usize,
) -> Vec<Line<'static>> {
    let max_cols = max_cols.max(8);
    let lines: Vec<Line<'static>> = if agents.is_empty() {
        vec![Line::from(Span::styled(
            "(no subagents this turn)",
            theme::panel(INSPECTOR).hint(),
        ))]
    } else {
        agents
            .iter()
            .enumerate()
            .map(|(idx, a)| {
                let mark = if idx == cursor { ">" } else { " " };
                let text = super::super::display_format::truncate_display_width(
                    &format!("{mark} {}  {}", a.id, a.status),
                    max_cols,
                );
                Line::from(Span::styled(
                    text,
                    if idx == cursor {
                        theme::panel(INSPECTOR).item(true)
                    } else {
                        theme::panel(INSPECTOR).item(false)
                    },
                ))
            })
            .collect()
    };
    let visible = height.max(4);
    let max_scroll = lines.len().saturating_sub(visible);
    let start = scroll.min(max_scroll);
    lines.into_iter().skip(start).take(visible).collect()
}
