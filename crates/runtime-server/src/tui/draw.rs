//! Ratatui rendering for the TUI shell.

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use super::layout::COMPOSER_FOOTER_ROWS;

use super::app::AppState;
use super::focus::FocusRegion;
use super::layout::{InspectorTab, LayoutRegions};
use super::overlay::{draw_approval, draw_help};
use super::theme;

pub fn draw(frame: &mut Frame<'_>, app: &AppState, regions: &LayoutRegions) {
    // Clear full frame first — prevents trailing chars when lines shrink between refreshes.
    frame.render_widget(Clear, frame.area());

    let layout = &app.layout;
    let workspace = &app.workspace_display;
    let focus = layout.focus;
    let border_focus = theme::border_focus();
    let border_idle = theme::border_idle();

    let left_style = if focus == FocusRegion::Left {
        border_focus
    } else {
        border_idle
    };
    let right_style = if focus == FocusRegion::Right {
        border_focus
    } else {
        border_idle
    };

    let fold_hint = format!(
        " [{}]L {} [[]]R ",
        if regions.left_visible { "v" } else { "^" },
        if regions.right_visible { "v" } else { "^" }
    );
    let stream = if app.transcript.is_thinking() {
        "thinking"
    } else if app.transcript.is_tools_active() {
        "tools"
    } else if app.transcript.streaming {
        "streaming"
    } else {
        "idle"
    };
    let workspace_short = truncate_middle(workspace, 28);
    let status_short = truncate_middle(&app.title_status_line(), 48);
    let title = Paragraph::new(format!(
        " Zagens TUI | {workspace_short} | {status_short}{fold_hint}| {stream} "
    ))
    .style(theme::title_bar());
    frame.render_widget(title, regions.title);

    if regions.left_visible {
        let tabs: String = InspectorTab::ALL
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mark = if layout.prefs.inspector_tab() == *t {
                    ">"
                } else {
                    " "
                };
                format!("{mark}{}{}", i + 1, t.label())
            })
            .collect::<Vec<_>>()
            .join(" ");
        let session_lines = app
            .sessions
            .render_lines(regions.left.height.saturating_sub(8) as usize);
        let body = format!(
            "Sessions\n{}\n\nInspector\n{tabs}\n\nj/k Enter Ctrl+N",
            session_lines.join("\n")
        );
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(left_style)
            .title(" Left ");
        frame.render_widget(
            Paragraph::new(body).block(block).wrap(Wrap { trim: false }),
            regions.left,
        );
    }

    let (transcript, composer) = layout.center_panes(regions.center);
    let chat_focused = focus == FocusRegion::Chat;
    let transcript_border = if chat_focused && !app.composer_focus {
        border_focus
    } else if chat_focused {
        border_idle
    } else {
        border_idle
    };
    let composer_border = if chat_focused && app.composer_focus {
        border_focus
    } else {
        border_idle
    };

    let transcript_block = Block::default()
        .borders(Borders::ALL)
        .border_style(transcript_border)
        .title(if chat_focused && !app.composer_focus {
            " Transcript (scroll) "
        } else {
            " Transcript "
        });
    let transcript_height = transcript.height.saturating_sub(2) as usize;
    let transcript_width = transcript.width.saturating_sub(2) as usize;
    let lines = app.transcript_render(transcript_height.max(4), transcript_width.max(20));
    frame.render_widget(Clear, transcript);
    frame.render_widget(Paragraph::new(lines).block(transcript_block), transcript);

    let composer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(composer_border)
        .title(if app.transcript.is_live_activity() {
            " Composer (waiting) "
        } else if app.composer_focus {
            " Composer "
        } else {
            " Composer (scroll) "
        });
    frame.render_widget(Clear, composer);
    let inner = composer_block.inner(composer);
    let composer_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(COMPOSER_FOOTER_ROWS)])
        .split(inner);
    let input_height = composer_areas[0].height as usize;
    let input_width = composer_areas[0].width as usize;
    let footer_width = composer_areas[1].width as usize;
    let composer_lines = app.composer_render(input_height.max(3), input_width.max(20));
    let footer_line = app.composer_footer_line(footer_width.max(20));
    frame.render_widget(Clear, composer_areas[0]);
    frame.render_widget(Paragraph::new(composer_lines), composer_areas[0]);
    frame.render_widget(Clear, composer_areas[1]);
    frame.render_widget(Paragraph::new(vec![footer_line]), composer_areas[1]);
    frame.render_widget(composer_block, composer);

    if regions.right_visible {
        let tab = layout.prefs.inspector_tab();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(right_style)
            .title(format!(" {} ", tab.label()));
        let panel_height = regions.right.height.saturating_sub(2) as usize;
        let lines = app.inspector.render(tab, panel_height.max(4));
        frame.render_widget(
            Paragraph::new(lines.join("\n"))
                .block(block)
                .wrap(Wrap { trim: false }),
            regions.right,
        );
    }

    if app.show_help {
        draw_help(frame);
    }
    if let Some(pending) = &app.pending_approval {
        draw_approval(frame, pending);
    }
}

fn truncate_middle(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars < 5 {
        return truncate_chars(text, max_chars);
    }
    let head = max_chars / 2 - 1;
    let tail = max_chars - head - 1;
    format!(
        "{}…{}",
        truncate_chars(text, head),
        text.chars()
            .rev()
            .take(tail)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
    )
}

fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}
