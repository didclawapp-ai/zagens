//! Tool approval modal (Phase 2).

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub id: String,
    pub tool_name: String,
    pub description: String,
    pub approval_key: String,
}

pub fn draw_approval(frame: &mut Frame<'_>, pending: &PendingApproval) {
    let area = centered_rect(70, 40, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Approval required ");
    let body = format!(
        "Tool: {}\n\n{}\n\n[y] Allow   [n] Deny   [a] Allow session\n[Esc] Deny",
        pending.tool_name, pending.description
    );
    frame.render_widget(
        Paragraph::new(body)
            .block(block)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::White)),
        area,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
