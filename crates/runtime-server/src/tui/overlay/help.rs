//! Static shortcut help overlay.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::super::theme;

pub const HELP_LINES: &str = r#"Zagens TUI — shortcuts

Focus
  Tab / Shift+Tab     Rotate Left · Chat · Right
  [ / ]               Collapse left / right rail

Left rail (sessions)
  j / k               Select session
  Enter               Switch session
  Ctrl+N              New session
  1-5                 Inspector tab shortcut

Chat
  Tab                 Input → scroll transcript → side columns
  Shift+Tab           Reverse focus order
  Esc                 Toggle input / scroll (cancel / menu when typing /)
  Enter               Send prompt (input mode)
  Ctrl+V              Paste from clipboard
  /commands           Slash menu — ↑↓ select · Enter run
  /model <id>         Switch text model (alias /m)
  ↑ / ↓ / j / k       Scroll transcript (auto-enter scroll mode)
  PgUp / PgDn         Scroll transcript (auto-enter scroll mode)
  Ctrl+A              Cycle approval policy (4 modes, saved to config)
  o                   Expand/collapse last tool block

Approval modal
  y                   Allow
  n / Esc             Deny
  a                   Allow for session

Global
  Ctrl+C              Interrupt turn
  Ctrl+C twice        Quit
  Ctrl+Q              Quit
  ?                   Toggle this help

Terminal font (recommended)
  Windows Terminal    Cascadia Mono, JetBrains Mono, Consolas
  Legacy console      Consolas 11+ or NSimSun for CJK
  Set in terminal profile — zagens-tui uses your terminal font
"#;

pub fn draw_help(frame: &mut Frame<'_>) {
    let area = centered_rect(75, 70, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border_focus())
        .style(theme::overlay_panel())
        .title(" Help (? to close) ");
    frame.render_widget(
        Paragraph::new(HELP_LINES)
            .block(block)
            .style(theme::approval_body()),
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
