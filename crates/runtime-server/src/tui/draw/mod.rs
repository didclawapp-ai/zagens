//! TUI frame orchestration — layout regions → panes → optional column dividers.

mod chrome;
mod pane;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, Paragraph};

use super::app::AppState;
use super::focus::FocusRegion;
use super::i18n::inspector_tab_hint;
use super::inspector::render_lht_styled;
use super::layout::{InspectorTab, LayoutRegions, RightPaneRegions, split_center_column};
use super::overlay::{draw_approval, draw_automation, draw_help, draw_onboarding};
use crate::localization::{MessageId, tr};

use super::theme::{self, TuiPanel};

use chrome::{
    BorderPlan, DividerStyles, paint_column_backgrounds, paint_column_dividers,
    seal_panel_edge_columns,
};
use pane::paint_pane;

struct BorderStyles {
    left: Style,
    right: Style,
    center: Style,
    transcript: Style,
    composer: Style,
}

pub fn draw(
    frame: &mut Frame<'_>,
    app: &mut AppState,
    regions: &LayoutRegions,
    right: &RightPaneRegions,
) {
    let _span = tracing::trace_span!("tui_draw").entered();
    let root_bg = theme::panel(TuiPanel::Transcript).surface(false);
    frame.render_widget(Clear, frame.area());
    frame.render_widget(Block::default().style(root_bg), frame.area());

    let borderless = theme::current().borderless();
    let styles = if borderless {
        None
    } else {
        Some(border_styles(app, regions))
    };
    draw_title_bar(frame, app, regions);
    if borderless {
        paint_column_backgrounds(frame.buffer_mut(), regions);
    }
    draw_left_rail(frame, app, regions, styles.as_ref());
    draw_right_rail(frame, app, regions, right, styles.as_ref());
    draw_center_column(frame, app, regions, styles.as_ref());
    if borderless {
        seal_panel_edge_columns(frame.buffer_mut(), regions);
    }

    if !borderless && let Some(styles) = styles {
        paint_column_dividers(
            frame.buffer_mut(),
            regions,
            &DividerStyles {
                left_center: styles.left,
                center_right: styles.right,
                center_outer: styles.center,
            },
        );
    }

    if app.show_help {
        draw_help(frame, app.locale);
    }
    if let Some(pending) = &app.pending_approval {
        draw_approval(frame, app.locale, pending);
    }
    if app.show_automation {
        draw_automation(
            frame,
            app.locale,
            &app.automation_engine.config,
            &app.automation_ui,
        );
    }
    if app.show_onboarding {
        draw_onboarding(frame, app.locale, &app.onboarding, &app.workspace_display);
    }
}

fn border_styles(app: &AppState, regions: &LayoutRegions) -> BorderStyles {
    let focus = app.layout.focus;
    let chat_focused = focus == FocusRegion::Chat;
    let border_focus = theme::border_focus();
    let border_idle = theme::border_idle();
    let border_focus_sidebar = theme::border_focus_sidebar();
    let border_idle_sidebar = theme::border_idle_sidebar();

    let left = if focus == FocusRegion::Left {
        border_focus_sidebar
    } else {
        border_idle_sidebar
    };
    let right = if focus == FocusRegion::Right {
        border_focus_sidebar
    } else {
        border_idle_sidebar
    };
    let transcript = if chat_focused && !app.composer_focus {
        border_focus
    } else {
        border_idle
    };
    let composer = if chat_focused && app.composer_focus {
        border_focus
    } else {
        border_idle
    };
    let center = if chat_focused { composer } else { border_idle };
    let _ = regions;
    BorderStyles {
        left,
        right,
        center,
        transcript,
        composer,
    }
}

fn draw_title_bar(frame: &mut Frame<'_>, app: &AppState, regions: &LayoutRegions) {
    let fold_hint = format!(
        " | [{}]L {} [[]]R ",
        if regions.left_visible { "v" } else { "^" },
        if regions.right_visible { "v" } else { "^" }
    );
    let workspace_short = truncate_middle(&app.workspace_display, 48);
    let title_text = pad_title_line(
        &format!(" Zagens TUI | {workspace_short}{fold_hint}"),
        regions.title.width as usize,
    );
    frame.render_widget(
        Paragraph::new(title_text).style(theme::title_bar()),
        regions.title,
    );
}

fn draw_center_column(
    frame: &mut Frame<'_>,
    app: &mut AppState,
    regions: &LayoutRegions,
    styles: Option<&BorderStyles>,
) {
    let live_activity = app.transcript.is_live_activity();
    // Activity strip removed: transcript THK/tool lines already convey the same state.
    // Pass false so no row is reserved for the strip; live_activity is kept for the
    // composer title ("waiting") and hint text only.
    let center = split_center_column(regions.center, false, app.layout.composer_lines);
    let chat_focused = app.layout.focus == FocusRegion::Chat;
    let section = BorderPlan::for_center_section();
    let borderless = theme::current().borderless();

    let transcript_focused = chat_focused && !app.composer_focus;
    let transcript_fill = theme::panel(TuiPanel::Transcript).surface(transcript_focused);
    let transcript_title = if transcript_focused {
        tr(app.locale, MessageId::TuiPanelTranscriptScroll)
    } else {
        tr(app.locale, MessageId::TuiPanelTranscript)
    };
    let (transcript_w, transcript_h) = pane::inset_line_budget(center.transcript, section);
    paint_pane(
        frame,
        center.transcript,
        transcript_title,
        section,
        pane_border_style(styles, TuiPanel::Transcript, transcript_focused),
        transcript_fill,
        app.transcript_render(transcript_h.max(4), transcript_w.max(20)),
        true,
    );

    let composer_focused = chat_focused && app.composer_focus;
    let composer_title = if live_activity {
        if composer_focused {
            tr(app.locale, MessageId::TuiPanelComposerWaitingEdit)
        } else {
            tr(app.locale, MessageId::TuiPanelComposerWaitingScroll)
        }
    } else if composer_focused {
        tr(app.locale, MessageId::TuiPanelComposer)
    } else {
        tr(app.locale, MessageId::TuiPanelComposerScroll)
    };
    let (input_w, composer_h) = pane::inset_line_budget(center.composer, section);
    let pending_preview: Vec<_> =
        if app.transcript.is_live_activity() && !app.prompt_queue.is_empty() {
            app.queued_preview_lines(input_w.max(20))
        } else {
            Vec::new()
        };
    let pending_rows = pending_preview.len().min(6);
    let palette_rows = if app.slash.open {
        app.slash_palette_lines(input_w.max(20), 6).len().min(6)
    } else {
        0
    };
    let text_h = composer_h
        .saturating_sub(palette_rows + pending_rows)
        .max(1);
    let mut composer_lines = pending_preview;
    composer_lines.extend(app.composer_render(text_h.max(1), input_w.max(20)));
    if app.slash.open && palette_rows > 0 {
        composer_lines.extend(app.slash_palette_lines(input_w.max(20), palette_rows));
    }
    paint_pane(
        frame,
        center.composer,
        composer_title,
        section,
        pane_border_style(styles, TuiPanel::Composer, composer_focused),
        theme::panel(TuiPanel::Composer).surface(composer_focused),
        composer_lines,
        true,
    );

    let status_title = tr(app.locale, MessageId::TuiPanelStatus);
    let (status_w, _) = pane::inset_line_budget(center.status, section);
    paint_pane(
        frame,
        center.status,
        status_title,
        section,
        pane_border_style(styles, TuiPanel::Status, false),
        theme::panel(TuiPanel::Status).surface(false),
        vec![app.composer_footer_line(status_w.max(20))],
        true,
    );

    let _ = borderless;
}

fn pane_border_style(styles: Option<&BorderStyles>, panel: TuiPanel, focused: bool) -> Style {
    if theme::current().borderless() {
        return theme::panel(panel).surface(focused);
    }
    let Some(styles) = styles else {
        return theme::border_idle();
    };
    match panel {
        TuiPanel::Transcript => styles.transcript,
        TuiPanel::Composer => styles.composer,
        _ => styles.composer,
    }
}

fn draw_left_rail(
    frame: &mut Frame<'_>,
    app: &AppState,
    regions: &LayoutRegions,
    styles: Option<&BorderStyles>,
) {
    if !regions.left_visible {
        return;
    }
    let left_focused = app.layout.focus == FocusRegion::Left;
    let plan = BorderPlan::for_left_rail();
    let (inner_w, inner_h) = pane::line_budget(regions.left, plan);
    let mut lines =
        app.sessions
            .render_styled_lines(inner_h.saturating_sub(6).max(4), inner_w, app.locale);
    lines.push(super::left_rail::SessionList::inspector_tab_line(
        app.layout.prefs.inspector_tab(),
        inner_w,
        app.locale,
    ));
    let border_style = styles.map_or(theme::panel(TuiPanel::Left).surface(left_focused), |s| {
        if left_focused {
            s.left
        } else {
            theme::border_idle_sidebar()
        }
    });
    paint_pane(
        frame,
        regions.left,
        tr(app.locale, MessageId::TuiPanelLeft),
        plan,
        border_style,
        theme::panel(TuiPanel::Left).surface(left_focused),
        lines,
        false,
    );
}

fn draw_right_rail(
    frame: &mut Frame<'_>,
    app: &mut AppState,
    regions: &LayoutRegions,
    right: &RightPaneRegions,
    styles: Option<&BorderStyles>,
) {
    if !regions.right_visible {
        return;
    }
    let focus = app.layout.focus;
    let plan = BorderPlan::for_right_rail();
    let tab = app.layout.prefs.inspector_tab();
    let inspector_focused = focus == FocusRegion::Right
        && (!app.lht_pane_visible()
            || app.right_subfocus == super::focus::RightSubfocus::Inspector);
    let inspector_title = inspector_title(app.locale, tab, inspector_focused);
    let inspector_border = styles.map_or(
        theme::panel(TuiPanel::Inspector).surface(inspector_focused),
        |s| {
            if inspector_focused {
                s.right
            } else {
                theme::border_idle_sidebar()
            }
        },
    );
    let (inspector_w, inspector_h) = pane::line_budget(right.inspector, plan);
    if app.layout.prefs.inspector_tab() == InspectorTab::Activity {
        app.sync_activity_scroll_tail();
    }
    let activity_events = app.transcript.harness_events();
    let lines = app.inspector.render_styled(
        tab,
        inspector_h,
        inspector_w,
        &app.inspector_ui,
        &app.workspace,
        &activity_events,
    );
    paint_pane(
        frame,
        right.inspector,
        inspector_title.as_str(),
        plan,
        inspector_border,
        theme::panel(TuiPanel::Inspector).surface(inspector_focused),
        lines,
        false,
    );

    if !right.lht_visible {
        return;
    }
    let lht_focused =
        focus == FocusRegion::Right && app.right_subfocus == super::focus::RightSubfocus::Lht;
    let lht_title = if lht_focused {
        tr(app.locale, MessageId::TuiPanelLhtFocused)
    } else {
        tr(app.locale, MessageId::TuiPanelLht)
    };
    let lht_border = styles.map_or(theme::panel(TuiPanel::Lht).surface(lht_focused), |s| {
        if lht_focused {
            s.right
        } else {
            theme::border_idle_sidebar()
        }
    });
    let (lht_w, lht_h) = pane::line_budget(right.lht, plan);
    let lht_lines = render_lht_styled(app.task_graph.as_ref(), lht_h, app.lht_ui.scroll, lht_w);
    paint_pane(
        frame,
        right.lht,
        lht_title,
        plan,
        lht_border,
        theme::panel(TuiPanel::Lht).surface(lht_focused),
        lht_lines,
        false,
    );
}

fn inspector_title(
    locale: crate::localization::Locale,
    tab: InspectorTab,
    focused: bool,
) -> String {
    use super::i18n::inspector_tab_label;
    if !focused {
        return format!(" {} ", inspector_tab_label(locale, tab));
    }
    let hint = inspector_tab_hint(locale, tab);
    format!(" {} | {hint} ", inspector_tab_label(locale, tab))
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

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::tui::app::live_activity_app_state_for_draw;
    use crate::tui::theme::{TuiTheme, install};

    fn draw_smoke(size: (u16, u16)) {
        install(TuiTheme::default_theme());
        let mut app = live_activity_app_state_for_draw();
        let backend = TestBackend::new(size.0, size.1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                let area = frame.area();
                app.layout.last_terminal_width = area.width;
                app.layout.apply_auto_collapse(area.width);
                let regions = app.layout.regions(area);
                let split = app
                    .layout
                    .split_right_pane(regions.right, app.lht_pane_visible());
                draw(frame, &mut app, &regions, &split);
            })
            .expect("draw frame");
    }

    #[test]
    fn draw_live_activity_default_terminal() {
        draw_smoke((120, 30));
    }

    #[test]
    fn draw_live_activity_narrow_terminal() {
        draw_smoke((80, 24));
    }
}
