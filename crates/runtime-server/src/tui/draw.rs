//! Ratatui rendering for the TUI shell.

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use super::display_format::fill_styled_lines;

use super::inspector::render_lht_styled;
use super::layout::{CENTER_CONTENT_PAD, COMPOSER_FOOTER_DIVIDER_ROWS, COMPOSER_FOOTER_ROWS};
use super::layout::{InspectorTab, RightPaneRegions};

use super::activity_strip;
use super::app::AppState;
use super::focus::FocusRegion;
use super::layout::LayoutRegions;
use super::overlay::{draw_approval, draw_help};
use super::theme;

fn paint_area(frame: &mut Frame<'_>, area: Rect, style: Style) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(Clear, area);
    frame.render_widget(Block::default().style(style), area);
}

fn fill_block_lines(
    lines: Vec<Line<'static>>,
    area: Rect,
    block: &Block<'_>,
    blank_style: Style,
) -> Vec<Line<'static>> {
    let inner = block.inner(area);
    fill_styled_lines(
        lines,
        inner.height.max(1) as usize,
        inner.width.max(1) as usize,
        blank_style,
    )
}

struct CenterColumn {
    transcript: Rect,
    after_transcript_div: Rect,
    activity: Option<Rect>,
    after_activity_div: Option<Rect>,
    composer_input: Rect,
    before_footer_div: Rect,
    footer: Rect,
    after_footer_div: Rect,
}

fn split_center_column(area: Rect, live_activity: bool, composer_lines: u16) -> CenterColumn {
    let footer_zone =
        COMPOSER_FOOTER_DIVIDER_ROWS + COMPOSER_FOOTER_ROWS + COMPOSER_FOOTER_DIVIDER_ROWS;
    let input_rows = composer_lines.saturating_sub(footer_zone).max(3);
    let mut constraints: Vec<Constraint> = vec![Constraint::Min(8), Constraint::Length(1)];
    if live_activity {
        constraints.push(Constraint::Length(1));
        constraints.push(Constraint::Length(1));
    }
    constraints.extend([
        Constraint::Length(input_rows),
        Constraint::Length(COMPOSER_FOOTER_DIVIDER_ROWS),
        Constraint::Length(COMPOSER_FOOTER_ROWS),
        Constraint::Length(COMPOSER_FOOTER_DIVIDER_ROWS),
    ]);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut idx = 0usize;
    let transcript = rows[idx];
    idx += 1;
    let after_transcript_div = rows[idx];
    idx += 1;
    let (activity, after_activity_div) = if live_activity {
        let act = rows[idx];
        idx += 1;
        let div = rows[idx];
        idx += 1;
        (Some(act), Some(div))
    } else {
        (None, None)
    };
    let composer_input = rows[idx];
    idx += 1;
    let before_footer_div = rows[idx];
    idx += 1;
    let footer = rows[idx];
    idx += 1;
    let after_footer_div = rows[idx];

    CenterColumn {
        transcript,
        after_transcript_div,
        activity,
        after_activity_div,
        composer_input,
        before_footer_div,
        footer,
        after_footer_div,
    }
}

fn draw_faint_divider(frame: &mut Frame<'_>, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(theme::border_idle())
            .style(theme::shell_main()),
        area,
    );
}

/// Repaint a one-column vertical border strip (fixes breaks when pane text bleeds sideways).
fn paint_vertical_border_strip(frame: &mut Frame<'_>, area: Rect, side: Borders, style: Style) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(Block::default().borders(side).border_style(style), area);
}

/// Repaint vertical pane borders — one column per boundary (no double strips).
fn repair_column_borders(
    frame: &mut Frame<'_>,
    regions: &LayoutRegions,
    left_style: Style,
    center_style: Style,
    right_style: Style,
) {
    if regions.left_visible && regions.center.width > 0 {
        paint_vertical_border_strip(
            frame,
            Rect {
                x: regions
                    .left
                    .x
                    .saturating_add(regions.left.width.saturating_sub(1)),
                y: regions.left.y,
                width: 1,
                height: regions.left.height,
            },
            Borders::RIGHT,
            left_style,
        );
    } else if regions.center.width > 1 {
        paint_vertical_border_strip(
            frame,
            Rect {
                x: regions.center.x,
                y: regions.center.y,
                width: 1,
                height: regions.center.height,
            },
            Borders::LEFT,
            center_style,
        );
    }

    if regions.right_visible && regions.center.width > 0 {
        paint_vertical_border_strip(
            frame,
            Rect {
                x: regions.right.x,
                y: regions.right.y,
                width: 1,
                height: regions.right.height,
            },
            Borders::LEFT,
            right_style,
        );
    } else if regions.center.width > 1 {
        paint_vertical_border_strip(
            frame,
            Rect {
                x: regions
                    .center
                    .x
                    .saturating_add(regions.center.width.saturating_sub(1)),
                y: regions.center.y,
                width: 1,
                height: regions.center.height,
            },
            Borders::RIGHT,
            center_style,
        );
    }
}

fn inset_content_area(area: Rect) -> Rect {
    let pad = CENTER_CONTENT_PAD;
    if area.width <= pad.saturating_mul(2) {
        return area;
    }
    Rect {
        x: area.x.saturating_add(pad),
        y: area.y,
        width: area.width.saturating_sub(pad * 2),
        height: area.height,
    }
}

pub fn draw(
    frame: &mut Frame<'_>,
    app: &mut AppState,
    regions: &LayoutRegions,
    right: &RightPaneRegions,
) {
    // Clear + paint black shell (shrunk terminals drop cells outside the new area).
    frame.render_widget(Clear, frame.area());
    frame.render_widget(Block::default().style(theme::shell_main()), frame.area());

    let focus = app.layout.focus;
    let workspace = &app.workspace_display;
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
    let title_text = pad_title_line(
        &format!(" Zagens TUI | {workspace_short} | {status_short}{fold_hint}| {stream} "),
        regions.title.width as usize,
    );
    let title = Paragraph::new(title_text).style(theme::title_bar());
    paint_area(frame, regions.title, theme::shell_main());
    frame.render_widget(title, regions.title);

    if regions.left_visible {
        let session_height = regions.left.height.saturating_sub(10) as usize;
        let mut lines = app.sessions.render_styled_lines(session_height.max(4));
        lines.push(super::left_rail::SessionList::inspector_tab_line(
            app.layout.prefs.inspector_tab(),
        ));
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(left_style)
            .style(theme::shell_sidebar())
            .title(" Left ");
        let filled = fill_block_lines(lines, regions.left, &block, theme::shell_sidebar());
        paint_area(frame, regions.left, theme::shell_sidebar());
        frame.render_widget(
            Paragraph::new(filled)
                .block(block)
                .wrap(Wrap { trim: false }),
            regions.left,
        );
    }

    let chat_focused = focus == FocusRegion::Chat;
    let live_activity = app.transcript.is_live_activity();
    let center_border = if chat_focused {
        if app.composer_focus {
            border_focus
        } else {
            border_idle
        }
    } else {
        border_idle
    };
    let transcript_border = if chat_focused && !app.composer_focus {
        border_focus
    } else {
        border_idle
    };

    paint_area(frame, regions.center, theme::shell_main());
    let center = split_center_column(regions.center, live_activity, app.layout.composer_lines);

    let transcript_block = Block::default()
        .borders(Borders::TOP)
        .border_style(transcript_border)
        .style(theme::shell_main())
        .title(if chat_focused && !app.composer_focus {
            " Transcript (scroll) "
        } else {
            " Transcript "
        });
    let transcript_area = inset_content_area(center.transcript);
    let transcript_inner = transcript_block.inner(transcript_area);
    let transcript_height = transcript_inner.height.max(1) as usize;
    let transcript_width = transcript_inner.width.max(1) as usize;
    let lines = app.transcript_render(transcript_height.max(4), transcript_width.max(20));
    let transcript_lines = fill_block_lines(
        lines,
        transcript_area,
        &transcript_block,
        theme::shell_main(),
    );
    paint_area(frame, center.transcript, theme::shell_main());
    frame.render_widget(
        Paragraph::new(transcript_lines).block(transcript_block),
        transcript_area,
    );

    draw_faint_divider(frame, center.after_transcript_div);

    if let Some(strip_area) = center.activity {
        let strip_content = inset_content_area(strip_area);
        let strip_line =
            activity_strip::render_activity_strip(&app.transcript, strip_content.width.max(1));
        paint_area(frame, strip_area, theme::shell_main());
        frame.render_widget(Paragraph::new(strip_line), strip_content);
        if let Some(div) = center.after_activity_div {
            draw_faint_divider(frame, div);
        }
    }

    let composer_title = if live_activity {
        " Composer (waiting) "
    } else if app.composer_focus {
        " Composer "
    } else {
        " Composer (scroll) "
    };
    let composer_border = if chat_focused && app.composer_focus {
        border_focus
    } else {
        border_idle
    };
    let composer_block = Block::default()
        .borders(Borders::TOP)
        .border_style(composer_border)
        .title(composer_title)
        .style(theme::shell_main());
    let composer_area = inset_content_area(center.composer_input);
    let composer_inner = composer_block.inner(composer_area);
    let input_width = composer_inner.width.max(1) as usize;
    let palette_rows = if app.slash.open {
        app.slash_palette_lines(input_width.max(20), 6).len().min(6)
    } else {
        0
    };
    let text_height = composer_inner
        .height
        .saturating_sub(palette_rows as u16)
        .max(2) as usize;

    let composer_lines = fill_styled_lines(
        app.composer_render(text_height.max(2), input_width.max(20)),
        composer_inner.height.max(1) as usize,
        input_width.max(20),
        theme::shell_main(),
    );
    paint_area(frame, center.composer_input, theme::shell_main());
    frame.render_widget(
        Paragraph::new(composer_lines).block(composer_block),
        composer_area,
    );
    if app.slash.open && palette_rows > 0 {
        let palette_area = Rect {
            x: composer_inner.x,
            y: composer_inner
                .y
                .saturating_add(composer_inner.height.saturating_sub(palette_rows as u16)),
            width: composer_inner.width,
            height: palette_rows as u16,
        };
        let palette = app.slash_palette_lines(input_width.max(20), palette_rows);
        paint_area(frame, palette_area, theme::shell_main());
        frame.render_widget(Paragraph::new(palette), palette_area);
    }

    draw_faint_divider(frame, center.before_footer_div);
    let footer_area = inset_content_area(center.footer);
    let footer_width = footer_area.width.max(1) as usize;
    let footer_line = app.composer_footer_line(footer_width.max(20));
    let footer_lines = fill_styled_lines(
        vec![footer_line],
        footer_area.height.max(1) as usize,
        footer_width.max(20),
        theme::shell_main(),
    );
    paint_area(frame, center.footer, theme::shell_main());
    frame.render_widget(Paragraph::new(footer_lines), footer_area);
    draw_faint_divider(frame, center.after_footer_div);

    if regions.right_visible {
        let tab = app.layout.prefs.inspector_tab();
        let inspector_height = right.inspector.height.saturating_sub(2) as usize;
        let inspector_focused = focus == FocusRegion::Right
            && (!app.lht_pane_visible()
                || app.right_subfocus == super::focus::RightSubfocus::Inspector);
        let title = inspector_title(tab, inspector_focused);
        let lines = app.inspector.render_styled(
            tab,
            inspector_height.max(4),
            &app.inspector_ui,
            &app.workspace,
        );
        let inspector_borders = if right.lht_visible {
            Borders::LEFT | Borders::TOP | Borders::RIGHT
        } else {
            Borders::ALL
        };
        let block = Block::default()
            .borders(inspector_borders)
            .border_style(if inspector_focused {
                right_style
            } else {
                border_idle_sidebar
            })
            .style(theme::shell_sidebar())
            .title(title);
        let filled = fill_block_lines(lines, right.inspector, &block, theme::shell_sidebar());
        paint_area(frame, right.inspector, theme::shell_sidebar());
        frame.render_widget(
            Paragraph::new(filled)
                .block(block)
                .wrap(Wrap { trim: false }),
            right.inspector,
        );

        if right.lht_visible {
            let lht_focused = focus == FocusRegion::Right
                && app.right_subfocus == super::focus::RightSubfocus::Lht;
            let lht_height = right.lht.height.saturating_sub(2) as usize;
            let lht_title = if lht_focused {
                " LHT | j/k scroll l toggle i inspector "
            } else {
                " LHT "
            };
            let lht_block = Block::default()
                .borders(Borders::LEFT | Borders::BOTTOM | Borders::RIGHT)
                .border_style(if lht_focused {
                    right_style
                } else {
                    border_idle_sidebar
                })
                .style(theme::shell_sidebar())
                .title(lht_title);
            let lht_inner = lht_block.inner(right.lht);
            let lht_lines = render_lht_styled(
                app.task_graph.as_ref(),
                lht_height.max(4),
                app.lht_ui.scroll,
                lht_inner.width.max(1) as usize,
            );
            let filled = fill_block_lines(lht_lines, right.lht, &lht_block, theme::shell_sidebar());
            paint_area(frame, right.lht, theme::shell_sidebar());
            frame.render_widget(
                Paragraph::new(filled)
                    .block(lht_block)
                    .wrap(Wrap { trim: false }),
                right.lht,
            );
        }
    }

    repair_column_borders(frame, regions, left_style, center_border, right_style);
    if app.show_help {
        draw_help(frame);
    }
    if let Some(pending) = &app.pending_approval {
        draw_approval(frame, pending);
    }
}

fn inspector_title(tab: InspectorTab, focused: bool) -> String {
    if !focused {
        return format!(" {} ", tab.label());
    }
    let hint = match tab {
        InspectorTab::Files => "j/k nav Enter file/dir Esc back",
        InspectorTab::Diff => "j/k nav Enter patch s staged Esc",
        InspectorTab::Agents => "j/k nav",
        InspectorTab::Mcp => "j/k nav Enter tools",
    };
    format!(" {} | {hint} ", tab.label())
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

fn pad_title_line(text: &str, width: usize) -> String {
    super::display_format::pad_line_display_width(text, width.max(1))
}
