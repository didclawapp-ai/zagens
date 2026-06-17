//! Tool approval modal (Phase 2).

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::localization::{Locale, MessageId, tr};

use super::super::i18n::approval_body;
use super::super::theme;
use super::centered_rect;

#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub id: String,
    pub tool_name: String,
    pub description: String,
    pub approval_key: String,
    pub show_detail: bool,
}

pub fn draw_approval(frame: &mut Frame<'_>, locale: Locale, pending: &PendingApproval) {
    let area = centered_rect(70, 40, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::approval_border())
        .style(theme::overlay_panel())
        .title(tr(locale, MessageId::TuiApprovalTitle));
    let body = approval_body(locale, pending);
    frame.render_widget(
        Paragraph::new(body)
            .block(block)
            .wrap(Wrap { trim: false })
            .style(theme::approval_body()),
        area,
    );
}
