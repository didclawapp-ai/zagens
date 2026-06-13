//! Static shortcut help overlay.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::super::theme;

pub const HELP_LINES: &str = r#"Zagens TUI — shortcuts

Focus
  Tab / Shift+Tab     Rotate Left · Chat · Right (Right lands on upper inspector)
  [ / ]               Collapse left / right rail

Left rail (sessions)
  j / k               Select session
  Enter               Switch session
  Ctrl+N              New session
  1-5                 Inspector tab shortcut

Right rail (inspector + LHT)
  Tab                 Focus right column
  1-4                 Files / Diff / Agents / MCP
  j / k               Scroll (LHT pane when expanded + focused)
  Enter               Files: preview / expand dir · Diff: patch · MCP: tools
  Esc                 Back from preview / diff detail
  - / =               Narrow / widen right rail (saved to tui-layout.toml)
  l                   Toggle LHT lower pane
  i                   Focus upper inspector (when LHT visible)
  Enter               Files: expand/collapse directory
  s                   Diff: toggle staged vs worktree

Chat
  Tab                 Input → scroll transcript → side columns
  Shift+Tab           Reverse focus order
  Esc                 Toggle input / scroll (cancel / menu when typing /)
  Enter               Send prompt (input mode)
  Ctrl+V              Paste from clipboard
  /commands           Slash menu — ↑↓ select · Enter run
  /model <id>         Switch text model (alias /m)
  /lht [auto|strict|off]  LHT composer mode (empty cycles)
  /theme [name]       Switch TUI color theme (empty cycles)
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

Launch (CLI)
  --fresh             New session; default resumes last session in workspace

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
