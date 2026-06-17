//! MCP `mcp.json` editor overlay (`/mcp`).

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::localization::{Locale, MessageId, tr};

use super::super::composer_editor::ComposerEditor;
use super::super::display_format::{
    composer_cursor_blink_on, display_width, pad_line_display_width, truncate_display_width,
    wrap_display_line,
};
use super::super::theme;
use super::centered_rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum McpOverlayFocus {
    #[default]
    Editor,
    Save,
    Cancel,
}

#[derive(Debug)]
pub struct McpConfigUiState {
    pub path_display: String,
    pub editor: ComposerEditor,
    pub scroll: usize,
    pub focus: McpOverlayFocus,
    pub error: Option<String>,
    pub busy: bool,
}

impl McpConfigUiState {
    pub fn new(path_display: String, text: String) -> Self {
        let mut editor = ComposerEditor::default();
        editor.set_text(text);
        Self {
            path_display,
            editor,
            scroll: 0,
            focus: McpOverlayFocus::Editor,
            error: None,
            busy: false,
        }
    }

    pub fn next_focus(&mut self, reverse: bool) {
        self.focus = if reverse {
            match self.focus {
                McpOverlayFocus::Editor => McpOverlayFocus::Cancel,
                McpOverlayFocus::Save => McpOverlayFocus::Editor,
                McpOverlayFocus::Cancel => McpOverlayFocus::Save,
            }
        } else {
            match self.focus {
                McpOverlayFocus::Editor => McpOverlayFocus::Save,
                McpOverlayFocus::Save => McpOverlayFocus::Cancel,
                McpOverlayFocus::Cancel => McpOverlayFocus::Editor,
            }
        };
    }

    pub fn prev_focus(&mut self) {
        self.next_focus(true);
    }
}

pub fn draw_mcp_config(
    frame: &mut Frame<'_>,
    locale: Locale,
    ui: &McpConfigUiState,
    cursor_blink_since: std::time::Instant,
) {
    let area = centered_rect(88, 82, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border_focus())
        .style(theme::overlay_panel())
        .title(format!(" {} ", tr(locale, MessageId::TuiMcpTitle)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(if ui.error.is_some() { 2 } else { 0 }),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(format!(
            "{} {}",
            tr(locale, MessageId::TuiMcpPathLabel),
            ui.path_display
        ))
        .style(theme::hint()),
        chunks[0],
    );

    let editor_h = chunks[1].height as usize;
    let editor_w = chunks[1].width as usize;
    let lines = render_editor_lines(ui, editor_h, editor_w, cursor_blink_since);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(theme::approval_body()),
        chunks[1],
    );

    if let Some(err) = &ui.error {
        frame.render_widget(
            Paragraph::new(err.clone()).style(Style::default().fg(Color::Red)),
            chunks[2],
        );
    }

    let save_label = tr(locale, MessageId::TuiMcpSave);
    let cancel_label = tr(locale, MessageId::TuiMcpCancel);
    let save = button_span(save_label, ui.focus == McpOverlayFocus::Save);
    let cancel = button_span(cancel_label, ui.focus == McpOverlayFocus::Cancel);
    frame.render_widget(
        Paragraph::new(Line::from(vec![save, Span::raw("   "), cancel])),
        chunks[3],
    );

    frame.render_widget(
        Paragraph::new(tr(locale, MessageId::TuiMcpFooter)).style(theme::hint()),
        chunks[4],
    );
}

fn button_span(label: &str, active: bool) -> Span<'static> {
    let text = format!("[ {label} ]");
    if active {
        Span::styled(text, theme::footer_chip(theme::footer_mode()))
    } else {
        Span::styled(text, theme::hint())
    }
}

fn render_editor_lines(
    ui: &McpConfigUiState,
    max_lines: usize,
    max_cols: usize,
    cursor_blink_since: std::time::Instant,
) -> Vec<Line<'static>> {
    let text = ui.editor.text();
    let cursor = ui.editor.cursor();
    let (cursor_line, cursor_col) = cursor_line_col(text, cursor);

    let wrapped: Vec<(usize, String)> = text
        .lines()
        .enumerate()
        .flat_map(|(src_line, line)| {
            wrap_display_line(line, max_cols)
                .into_iter()
                .map(move |w| (src_line, w))
        })
        .collect();

    let total_lines = wrapped.len().max(1);
    let mut scroll = ui.scroll;
    if cursor_line < scroll {
        scroll = cursor_line;
    }
    let visible_h = max_lines.max(1);
    if cursor_line >= scroll + visible_h {
        scroll = cursor_line.saturating_sub(visible_h - 1);
    }
    scroll = scroll.min(total_lines.saturating_sub(1));

    let show_cursor = ui.focus == McpOverlayFocus::Editor;
    let cursor_on = show_cursor && composer_cursor_blink_on(cursor_blink_since);

    let end = (scroll + visible_h).min(total_lines);
    let mut out = Vec::new();
    for (idx, (src_line, line)) in wrapped.iter().enumerate().skip(scroll).take(end - scroll) {
        let mut content = line.clone();
        if show_cursor && cursor_on && *src_line == cursor_line {
            content = inject_cursor(content, cursor_col, max_cols);
        }
        let style = if ui.focus == McpOverlayFocus::Editor {
            theme::composer_idle()
        } else {
            theme::approval_body()
        };
        out.push(Line::from(Span::styled(
            pad_line_display_width(&content, max_cols),
            style,
        )));
        let _ = idx;
    }
    if out.is_empty() {
        let caret = if show_cursor && cursor_on { "-" } else { " " };
        out.push(Line::from(Span::styled(
            pad_line_display_width(caret, max_cols),
            theme::composer_idle(),
        )));
    }
    out
}

fn cursor_line_col(text: &str, cursor: usize) -> (usize, usize) {
    let cursor = cursor.min(text.len());
    let before = &text[..cursor];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = before[line_start..].chars().count();
    (line, col)
}

fn inject_cursor(mut line: String, col: usize, max_cols: usize) -> String {
    let byte_idx = line
        .char_indices()
        .nth(col)
        .map(|(i, _)| i)
        .unwrap_or_else(|| line.len());
    if display_width(&line) + 1 > max_cols {
        line = truncate_display_width(&line, max_cols.saturating_sub(1));
    }
    line.insert(byte_idx.min(line.len()), '█');
    line
}
