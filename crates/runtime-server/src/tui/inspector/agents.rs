//! Sub-agent list from cached engine events.

use ratatui::text::{Line, Span};

use super::super::theme;

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
) -> Vec<Line<'static>> {
    let lines: Vec<Line<'static>> = if agents.is_empty() {
        vec![Line::from(Span::styled(
            "(no subagents this turn)",
            theme::sidebar_hint(),
        ))]
    } else {
        agents
            .iter()
            .enumerate()
            .map(|(idx, a)| {
                let mark = if idx == cursor { ">" } else { " " };
                Line::from(Span::styled(
                    format!("{mark} {}  {}", a.id, a.status),
                    if idx == cursor {
                        theme::sidebar_item(true)
                    } else {
                        theme::sidebar_item(false)
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
