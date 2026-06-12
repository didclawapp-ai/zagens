//! Ratatui rendering for the TUI shell.

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::layout::COMPOSER_FOOTER_ROWS;

use super::activity_strip;
use super::app::AppState;
use super::focus::FocusRegion;
use super::layout::LayoutRegions;
use super::overlay::{draw_approval, draw_help};
use super::theme;

fn paint_area(frame: &mut Frame<'_>, area: Rect, style: Style) {
    frame.render_widget(Block::default().style(style), area);
}

pub fn draw(frame: &mut Frame<'_>, app: &AppState, regions: &LayoutRegions) {
    // Paint Dracula main background — do not use bare `Clear` (terminal default black).
    frame.render_widget(Block::default().style(theme::shell_main()), frame.area());

    let layout = &app.layout;
    let workspace = &app.workspace_display;
    let focus = layout.focus;
    let border_focus = theme::border_focus();
    let border_idle = theme::border_idle();
    let border_focus_sidebar = theme::border_focus_sidebar();
    let border_idle_sidebar = theme::border_idle_sidebar();

    let left_style = if focus == FocusRegion::Left {
        border_focus_sidebar
    } else {
        border_idle_sidebar
    };
    let right_style = if focus == FocusRegion::Right {
        border_focus_sidebar
    } else {
        border_idle_sidebar
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
        let session_height = regions.left.height.saturating_sub(10) as usize;
        let mut lines = app.sessions.render_styled_lines(session_height.max(4));
        lines.push(super::left_rail::SessionList::inspector_tab_line(
            layout.prefs.inspector_tab(),
        ));
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(left_style)
            .style(theme::shell_sidebar())
            .title(" Left ");
        frame.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false }),
            regions.left,
        );
    }

    let (transcript, activity, composer) = if app.transcript.is_live_activity() {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(8),
                Constraint::Length(1),
                Constraint::Length(layout.composer_lines),
            ])
            .split(regions.center);
        (rows[0], Some(rows[1]), rows[2])
    } else {
        let (t, c) = layout.center_panes(regions.center);
        (t, None, c)
    };
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
        .style(theme::shell_main())
        .title(if chat_focused && !app.composer_focus {
            " Transcript (scroll) "
        } else {
            " Transcript "
        });
    let transcript_height = transcript.height.saturating_sub(2) as usize;
    let transcript_width = transcript.width.saturating_sub(2) as usize;
    let lines = app.transcript_render(transcript_height.max(4), transcript_width.max(20));
    paint_area(frame, transcript, theme::shell_main());
    frame.render_widget(Paragraph::new(lines).block(transcript_block), transcript);

    if let Some(strip) = activity {
        let strip_line = activity_strip::render_activity_strip(&app.transcript, strip.width);
        paint_area(frame, strip, theme::shell_main());
        frame.render_widget(Paragraph::new(strip_line), strip);
    }

    let composer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(composer_border)
        .style(theme::shell_main())
        .title(if app.transcript.is_live_activity() {
            " Composer (waiting) "
        } else if app.composer_focus {
            " Composer "
        } else {
            " Composer (scroll) "
        });
    paint_area(frame, composer, theme::shell_main());
    let inner = composer_block.inner(composer);
    let composer_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(COMPOSER_FOOTER_ROWS)])
        .split(inner);
    let input_height = composer_areas[0].height as usize;
    let input_width = composer_areas[0].width as usize;
    let footer_width = composer_areas[1].width as usize;

    let palette_rows = if app.slash.open {
        app.slash_palette_lines(input_width.max(20), 6).len().min(6)
    } else {
        0
    };
    let text_height = input_height.saturating_sub(palette_rows).max(2);

    let composer_lines = app.composer_render(text_height.max(2), input_width.max(20));
    let footer_line = app.composer_footer_line(footer_width.max(20));
    paint_area(frame, composer_areas[0], theme::shell_main());
    frame.render_widget(Paragraph::new(composer_lines), composer_areas[0]);
    if app.slash.open && palette_rows > 0 {
        let palette_area = Rect {
            x: composer_areas[0].x,
            y: composer_areas[0]
                .y
                .saturating_add(composer_areas[0].height.saturating_sub(palette_rows as u16)),
            width: composer_areas[0].width,
            height: palette_rows as u16,
        };
        let palette = app.slash_palette_lines(input_width.max(20), palette_rows);
        paint_area(frame, palette_area, theme::shell_main());
        frame.render_widget(Paragraph::new(palette), palette_area);
    }
    paint_area(frame, composer_areas[1], theme::shell_main());
    frame.render_widget(Paragraph::new(vec![footer_line]), composer_areas[1]);
    frame.render_widget(composer_block, composer);

    if regions.right_visible {
        let tab = layout.prefs.inspector_tab();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(right_style)
            .style(theme::shell_sidebar())
            .title(format!(" {} ", tab.label()));
        let panel_height = regions.right.height.saturating_sub(2) as usize;
        let lines = app.inspector.render_styled(tab, panel_height.max(4));
        frame.render_widget(
            Paragraph::new(lines)
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
