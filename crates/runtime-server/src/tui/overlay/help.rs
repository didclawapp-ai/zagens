//! Static shortcut help overlay.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::localization::Locale;

use super::super::i18n::build_help_text;
use super::super::theme;
use super::centered_rect;
use crate::localization::{MessageId, tr};

pub fn draw_help(frame: &mut Frame<'_>, locale: Locale) {
    let area = centered_rect(75, 70, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border_focus())
        .style(theme::overlay_panel())
        .title(tr(locale, MessageId::TuiHelpCloseTitle));
    frame.render_widget(
        Paragraph::new(build_help_text(locale))
            .block(block)
            .style(theme::approval_body()),
        area,
    );
}
