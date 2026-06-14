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
    expanded: Option<usize>,
    max_cols: usize,
) -> Vec<Line<'static>> {
    let max_cols = max_cols.max(8);
    let lines: Vec<Line<'static>> = if agents.is_empty() {
        vec![Line::from(Span::styled(
            "(no subagents this turn)",
            theme::panel(INSPECTOR).hint(),
        ))]
    } else {
        let fmt = super::super::display_format::truncate_display_width;
        agents
            .iter()
            .enumerate()
            .flat_map(|(idx, a)| {
                let selected = idx == cursor;
                let mark = if selected { ">" } else { " " };
                let row = fmt(&format!("{mark} {}  {}", a.id, a.status), max_cols);
                let style = theme::panel(INSPECTOR).item(selected);
                let mut out: Vec<Line<'static>> = vec![Line::from(Span::styled(row, style))];
                // Expanded detail: show full id + status on separate indented lines.
                if expanded == Some(idx) {
                    let id_line = fmt(&format!("  id     {}", a.id), max_cols);
                    let st_line = fmt(&format!("  status {}", a.status), max_cols);
                    let dim = theme::panel(INSPECTOR).hint();
                    out.push(Line::from(Span::styled(id_line, dim)));
                    out.push(Line::from(Span::styled(st_line, dim)));
                }
                out
            })
            .collect()
    };
    let visible = height.max(4);
    let max_scroll = lines.len().saturating_sub(visible);
    let start = scroll.min(max_scroll);
    lines.into_iter().skip(start).take(visible).collect()
}
