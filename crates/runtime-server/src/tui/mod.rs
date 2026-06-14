//! Full-screen terminal UI (`zagens-tui`).

mod activity_strip;
mod app;
mod approval_policy;
mod composer_editor;
mod composer_paste;
mod composer_slash;
mod display_format;
mod draw;
mod focus;
mod harness;
mod inline_markdown;
mod input_thread;
pub(crate) mod inspector;
mod layout;
mod left_rail;
mod lht_mode;
mod markdown_table;
mod overlay;
mod poll;
mod runtime_events;
mod session_host;
mod stderr_log;
mod task_graph;
mod terminal;
mod theme;
mod transcript;
mod transcript_filter;
mod transcript_history;
mod transcript_turn;

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseEventKind};

use self::app::AppState;
use self::composer_slash::SlashAction;
use self::focus::FocusRegion;
use self::input_thread::TerminalInput;
use self::layout::{InspectorTab, TuiLayoutPrefs};
use self::session_host::TuiSessionHost;
use self::terminal::{TuiTerminal, is_ctrl_c, is_key_press};
use crate::cli::args::Cli;
use crate::cli::context::load_cli_context;

/// Entry point for the `zagens-tui` binary.
pub async fn run_tui(cli: Cli) -> Result<()> {
    if cli.command.is_some() {
        bail!(
            "zagens-tui does not run CLI subcommands; use `zagens` for exec/serve/doctor, or run `zagens-tui` without a subcommand"
        );
    }

    let mut ctx = load_cli_context(&cli)?;
    let mut host = TuiSessionHost::open(&mut ctx, &cli).await?;
    let resumed = cli.resume.is_some() || cli.continue_session || !cli.fresh;
    let initial_prompt = cli.prompt.clone();

    let inline_mode = cli.no_alt_screen;
    let mouse_capture = resolve_mouse_capture(&cli);
    let mut app = AppState::new(TuiLayoutPrefs::load(), inline_mode, &host).await;
    if resumed {
        app.seed_resume_banner();
    }
    app.schedule_next_poll();

    let mut tui = TuiTerminal::new(inline_mode, mouse_capture)?;
    terminal::sync_terminal_geometry(&mut tui.terminal, &mut app.layout)?;
    app.terminal_resized = true;
    let mut boot_paints_remaining = 1u8;
    let mut input = TerminalInput::spawn();
    let mut ctrl_c_streak = 0u8;
    let mut ctrl_c_last: Option<Instant> = None;
    let mut dirty = true;
    let mut anim_tick = tokio::time::interval(Duration::from_millis(120));
    anim_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut poll_wake = tokio::time::interval(Duration::from_secs(5));
    poll_wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    poll_wake.reset();

    if let Some(prompt) = initial_prompt.filter(|p| !p.trim().is_empty()) {
        submit_prompt(&host, &mut app, &prompt).await;
        dirty = true;
    }

    loop {
        if dirty {
            if app.terminal_resized || boot_paints_remaining > 0 {
                tui.terminal.clear()?;
                app.terminal_resized = false;
            }
            tui.terminal.draw(|frame| {
                let area = frame.area();
                app.layout.last_terminal_width = area.width;
                app.layout.apply_auto_collapse(area.width);
                let regions = app.layout.regions(area);
                app.right_panel_height = regions.right.height.saturating_sub(2) as usize;
                let split = app
                    .layout
                    .split_right_pane(regions.right, app.lht_pane_visible());
                app.right_inspector_height = split.inspector.height.saturating_sub(2) as usize;
                app.right_lht_height = if split.lht_visible {
                    split.lht.height.saturating_sub(2) as usize
                } else {
                    0
                };
                draw::draw(frame, &mut app, &regions, &split);
            })?;
            if boot_paints_remaining > 0 {
                boot_paints_remaining -= 1;
                if boot_paints_remaining > 0 {
                    app.terminal_resized = true;
                }
                dirty = true;
                continue;
            }
            dirty = false;
        }

        tokio::select! {
            delta = host.recv_runtime_ui_delta() => {
                let was_streaming = app.transcript.streaming;
                let had_events = !delta.events.is_empty();
                let had_checklist = delta.checklist.is_some() || delta.task_graph.is_some();
                for event in delta.events {
                    app.apply_engine_event(event);
                }
                if let Some(graph) = delta.task_graph {
                    app.apply_task_graph_snapshot(graph);
                } else if let Some(checklist) = delta.checklist {
                    app.merge_checklist_snapshot(checklist);
                }
                if was_streaming && !app.transcript.streaming {
                    app.refresh_workspace_inspector(&host);
                    drain_prompt_queue(&host, &mut app).await;
                }
                let poll_ran = app.poll_due();
                if poll_ran {
                    app.refresh_panels(&host).await;
                    app.refresh_sessions(&host).await;
                }
                if had_events
                    || had_checklist
                    || was_streaming != app.transcript.streaming
                    || poll_ran
                {
                    dirty = true;
                }
            }
            maybe_event = input.recv() => {
                if let Some(event) = maybe_event {
                    if handle_input_event(
                        &event,
                        &mut ctx,
                        &mut host,
                        &mut app,
                        &mut ctrl_c_streak,
                        &mut ctrl_c_last,
                    ).await? {
                        break;
                    }
                    dirty = true;
                }
            }
            _ = anim_tick.tick(), if app.transcript.is_live_activity() || app.composer_shows_cursor() => {
                dirty = true;
            }
            _ = poll_wake.tick() => {
                if app.poll_due() {
                    app.refresh_panels(&host).await;
                    app.refresh_sessions(&host).await;
                    dirty = true;
                }
            }
        }
    }

    app.layout.prefs.last_thread_id = Some(host.thread_id().to_string());
    app.layout.prefs.save().ok();
    tui.shutdown()?;
    Ok(())
}

async fn handle_input_event(
    event: &Event,
    ctx: &mut crate::cli::context::CliContext,
    host: &mut TuiSessionHost,
    app: &mut AppState,
    ctrl_c_streak: &mut u8,
    ctrl_c_last: &mut Option<Instant>,
) -> Result<bool> {
    match event {
        Event::Resize(width, _) => {
            app.layout.last_terminal_width = *width;
            app.layout.apply_auto_collapse(*width);
            app.terminal_resized = true;
        }
        Event::Paste(text) => {
            app.handle_composer_paste(text);
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => handle_mouse_scroll(app, mouse.column, 3, true),
            MouseEventKind::ScrollDown => handle_mouse_scroll(app, mouse.column, 3, false),
            _ => {}
        },
        Event::Key(key) => {
            if !is_key_press(key) {
                return Ok(false);
            }
            if app.show_help && key.code == KeyCode::Char('?') {
                app.show_help = false;
                return Ok(false);
            }
            if app.show_help {
                app.show_help = false;
                return Ok(false);
            }

            if let Some(pending) = app.pending_approval.clone() {
                return handle_approval_key(*key, host, app, &pending).await;
            }

            if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(true);
            }
            if is_ctrl_c(event) {
                let now = Instant::now();
                if ctrl_c_last.is_some_and(|t| now.duration_since(t) > Duration::from_millis(1500))
                {
                    *ctrl_c_streak = 0;
                }
                *ctrl_c_streak = ctrl_c_streak.saturating_add(1);
                *ctrl_c_last = Some(now);
                if *ctrl_c_streak >= 2 {
                    return Ok(true);
                }
                host.interrupt_turn().await?;
                app.transcript.close_open_turn();
                app.blocked_line = None;
                return Ok(false);
            }
            *ctrl_c_streak = 0;
            *ctrl_c_last = None;

            match key.code {
                KeyCode::Char('?') => {
                    app.show_help = !app.show_help;
                }
                KeyCode::Tab if !app.approval_open() => {
                    handle_tab_focus(app, key.modifiers.contains(KeyModifiers::SHIFT));
                }
                KeyCode::Char('[') => app.layout.toggle_left(),
                KeyCode::Char(']') => app.layout.toggle_right(),
                KeyCode::Char('n')
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && app.layout.focus == FocusRegion::Left =>
                {
                    host.new_session(ctx).await?;
                    app.reload_after_thread_switch(host).await;
                }
                KeyCode::Char('j')
                    if app.layout.focus == FocusRegion::Chat && app.composer_focus =>
                {
                    app.handle_char('j');
                }
                KeyCode::Char('k')
                    if app.layout.focus == FocusRegion::Chat && app.composer_focus =>
                {
                    app.handle_char('k');
                }
                KeyCode::Char('j') => {
                    enter_transcript_scroll_if_needed(app);
                    handle_j(app);
                }
                KeyCode::Char('k') => {
                    enter_transcript_scroll_if_needed(app);
                    handle_k(app);
                }
                KeyCode::Char('o')
                    if app.layout.focus == FocusRegion::Chat && !app.composer_focus =>
                {
                    if let Some(src) = app.transcript.last_mermaid_src() {
                        if let Some(url) = mermaid_live_url(src) {
                            let _ = open_url(&url);
                        }
                    } else if app.transcript.last_turn_has_harness() {
                        app.transcript.toggle_last_turn_harness();
                    } else {
                        app.transcript.toggle_last_turn_tools();
                    }
                }
                KeyCode::Char(n @ '1'..='5')
                    if app.layout.focus == FocusRegion::Left
                        || app.layout.focus == FocusRegion::Right =>
                {
                    if let Some(tab) = InspectorTab::from_index(n as u8 - b'0') {
                        app.switch_inspector_tab(tab);
                        app.focus_inspector_upper();
                    }
                }
                KeyCode::Char('l') if app.layout.focus == FocusRegion::Right => {
                    app.toggle_lht_pane();
                }
                KeyCode::Char('i') if app.layout.focus == FocusRegion::Right => {
                    app.focus_inspector_upper();
                }
                KeyCode::Enter if app.layout.focus == FocusRegion::Right => {
                    app.inspector_activate(&host.thread.workspace);
                }
                KeyCode::Esc if app.layout.focus == FocusRegion::Right => {
                    if app.inspector_ui.in_detail_view() || app.inspector_ui.mcp_expanded.is_some()
                    {
                        app.inspector_back();
                    }
                }
                KeyCode::Char('s')
                    if app.layout.focus == FocusRegion::Right
                        && app.layout.prefs.inspector_tab() == InspectorTab::Diff =>
                {
                    app.toggle_diff_staged(&host.thread.workspace);
                }
                KeyCode::Char('-') | KeyCode::Char('_')
                    if app.layout.focus == FocusRegion::Right =>
                {
                    app.layout.adjust_right_width(-2);
                }
                KeyCode::Char('=') | KeyCode::Char('+')
                    if app.layout.focus == FocusRegion::Right =>
                {
                    app.layout.adjust_right_width(2);
                }
                KeyCode::Esc if app.layout.focus == FocusRegion::Chat => {
                    if app.composer_focus && app.slash.open {
                        // Restore theme if the /theme picker was open in preview mode.
                        if let Some(original) = app.theme_picker_original.take() {
                            theme::install(theme::TuiTheme::resolve(original));
                        }
                        app.composer.clear();
                        app.slash.close();
                    } else {
                        app.composer_focus = !app.composer_focus;
                    }
                }
                KeyCode::Enter if app.layout.focus == FocusRegion::Left => {
                    if let Some(id) = app.sessions.selected_id()
                        && id != app.thread_id
                    {
                        host.switch_thread(id).await?;
                        app.reload_after_thread_switch(host).await;
                    }
                }
                KeyCode::Enter if app.layout.focus == FocusRegion::Chat => {
                    if app.composer_focus {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            app.handle_newline();
                        } else if app.slash.open
                            || composer_slash::composer_is_slash_command(app.composer.text())
                        {
                            if handle_slash_enter(ctx, host, app).await? {
                                return Ok(false);
                            }
                        } else if app.can_send_prompt()
                            && let Some(prompt) = app.take_composer_prompt()
                        {
                            submit_prompt(host, app, &prompt).await;
                        }
                    } else {
                        app.transcript.toggle_last_turn_detail();
                    }
                }
                KeyCode::Left if app.layout.focus == FocusRegion::Chat && app.composer_focus => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        app.composer.move_word_left();
                    } else {
                        app.composer.move_left();
                    }
                }
                KeyCode::Right if app.layout.focus == FocusRegion::Chat && app.composer_focus => {
                    app.composer.move_right();
                }
                KeyCode::Home if app.layout.focus == FocusRegion::Chat && app.composer_focus => {
                    app.composer.move_home();
                }
                KeyCode::End if app.layout.focus == FocusRegion::Chat && app.composer_focus => {
                    app.composer.move_end();
                }
                KeyCode::Char('w')
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && app.layout.focus == FocusRegion::Chat
                        && app.composer_focus =>
                {
                    app.composer.delete_word_backward();
                    app.sync_slash_palette();
                }
                KeyCode::Char('u')
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && app.layout.focus == FocusRegion::Chat
                        && app.composer_focus =>
                {
                    app.composer.delete_to_start();
                    app.sync_slash_palette();
                }
                KeyCode::Up
                    if app.layout.focus == FocusRegion::Chat
                        && app.composer_focus
                        && app.slash.open =>
                {
                    app.slash.move_up(app.composer.text(), &app.model_catalog);
                    preview_theme_selection(app);
                }
                KeyCode::Down
                    if app.layout.focus == FocusRegion::Chat
                        && app.composer_focus
                        && app.slash.open =>
                {
                    app.slash.move_down(app.composer.text(), &app.model_catalog);
                    preview_theme_selection(app);
                }
                KeyCode::Up
                    if app.layout.focus == FocusRegion::Chat
                        && app.composer_focus
                        && !app.slash.open =>
                {
                    // Multi-line: move cursor up within text; fall back to history at first line.
                    if !app.composer.move_up_line() {
                        app.prompt_history.browse_up(&mut app.composer);
                        app.sync_slash_palette();
                    }
                }
                KeyCode::Down
                    if app.layout.focus == FocusRegion::Chat
                        && app.composer_focus
                        && !app.slash.open =>
                {
                    // Multi-line: move cursor down within text; fall back to history at last line.
                    if !app.composer.move_down_line() {
                        app.prompt_history.browse_down(&mut app.composer);
                        app.sync_slash_palette();
                    }
                }
                KeyCode::Up if app.layout.focus == FocusRegion::Chat => {
                    enter_transcript_scroll_if_needed(app);
                    app.transcript.scroll_up(1);
                }
                KeyCode::Down if app.layout.focus == FocusRegion::Chat => {
                    enter_transcript_scroll_if_needed(app);
                    app.transcript.scroll_down(1);
                }
                KeyCode::PageUp if app.layout.focus == FocusRegion::Chat => {
                    enter_transcript_scroll_if_needed(app);
                    app.transcript.scroll_up(5);
                }
                KeyCode::PageDown if app.layout.focus == FocusRegion::Chat => {
                    enter_transcript_scroll_if_needed(app);
                    app.transcript.scroll_down(5);
                }
                KeyCode::Char('a')
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && app.layout.focus == FocusRegion::Chat
                        && app.approval_toggle_enabled =>
                {
                    host.cycle_approval_policy().await?;
                    app.sync_thread_meta(host);
                }
                KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.paste_from_clipboard();
                }
                KeyCode::Backspace => app.handle_backspace(),
                KeyCode::Delete if app.layout.focus == FocusRegion::Chat && app.composer_focus => {
                    app.composer.delete_forward();
                    app.sync_slash_palette();
                }
                KeyCode::Char(ch) => app.handle_char(ch),
                _ => {}
            }
        }
        _ => {}
    }
    Ok(false)
}

fn enter_transcript_scroll_if_needed(app: &mut AppState) {
    if app.layout.focus == FocusRegion::Chat && app.composer_focus {
        app.composer_focus = false;
    }
}

fn handle_tab_focus(app: &mut AppState, shift: bool) {
    let sidebars = app.layout.left_rail_available() || app.layout.right_rail_available();
    if app.layout.focus == FocusRegion::Chat {
        if !sidebars {
            app.composer_focus = !app.composer_focus;
            return;
        }
        if !shift && app.composer_focus {
            app.composer_focus = false;
            return;
        }
        if shift && !app.composer_focus {
            app.composer_focus = true;
            return;
        }
    }
    let prev_focus = app.layout.focus;
    app.layout.focus = if shift {
        app.layout.focus_prev_visible()
    } else {
        app.layout.focus_next_visible()
    };
    if app.layout.focus == FocusRegion::Right && prev_focus != FocusRegion::Right {
        app.focus_inspector_upper();
    }
    if app.layout.focus == FocusRegion::Chat {
        app.composer_focus = !shift;
    }
}

fn handle_j(app: &mut AppState) {
    match app.layout.focus {
        FocusRegion::Left => app.sessions.move_down(),
        FocusRegion::Chat if !app.composer_focus => app.transcript.scroll_up(1),
        FocusRegion::Right => {
            let ws = app.workspace.clone();
            app.right_rail_scroll_down(&ws);
        }
        _ => {}
    }
}

fn handle_k(app: &mut AppState) {
    match app.layout.focus {
        FocusRegion::Left => app.sessions.move_up(),
        FocusRegion::Chat if !app.composer_focus => app.transcript.scroll_down(1),
        FocusRegion::Right => {
            let ws = app.workspace.clone();
            app.right_rail_scroll_up(&ws);
        }
        _ => {}
    }
}

/// Route a mouse-wheel scroll to whichever pane contains column `x`.
/// `up=true` means wheel-up (scroll toward older/top content).
/// `lines` is how many lines to scroll per tick.
fn handle_mouse_scroll(app: &mut AppState, x: u16, lines: usize, up: bool) {
    let total = app.layout.last_terminal_width;
    let left_w = app.layout.left_width();
    let right_w = app.layout.right_width();

    if left_w > 0 && x < left_w {
        // Left rail — session list: 1 session per tick to avoid skipping entries.
        if up {
            app.sessions.move_up();
        } else {
            app.sessions.move_down();
        }
    } else if right_w > 0 && x + right_w >= total {
        // Right rail — file tree / inspector: 1 line per tick so sub-directory
        // items don't jump when the tree is expanded.
        let ws = app.workspace.clone();
        if up {
            app.right_rail_scroll_up(&ws);
        } else {
            app.right_rail_scroll_down(&ws);
        }
    } else {
        // Center column — transcript: 3 lines per tick for comfortable reading.
        if up {
            app.transcript.scroll_up(lines);
        } else {
            app.transcript.scroll_down(lines);
        }
    }
}

async fn handle_approval_key(
    key: crossterm::event::KeyEvent,
    host: &mut TuiSessionHost,
    app: &mut AppState,
    pending: &overlay::PendingApproval,
) -> Result<bool> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            host.approve_tool(&pending.id, false).await?;
            app.clear_approval();
        }
        KeyCode::Char('a') => {
            host.approve_tool(&pending.id, true).await?;
            app.clear_approval();
        }
        KeyCode::Char('v') => {
            if let Some(p) = app.pending_approval.as_mut() {
                p.show_detail = !p.show_detail;
            }
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            host.deny_tool(&pending.id).await?;
            app.clear_approval();
        }
        _ => {}
    }
    Ok(false)
}

async fn handle_slash_enter(
    ctx: &mut crate::cli::context::CliContext,
    host: &mut TuiSessionHost,
    app: &mut AppState,
) -> Result<bool> {
    let current_ws = host.thread.workspace.clone();
    if composer_slash::theme_picker_active(app.composer.text())
        && app.slash.open
        && let Some(theme_id) =
            composer_slash::selected_theme(app.composer.text(), app.slash.selected)
    {
        app.composer.clear();
        app.slash.close();
        // Theme is already applied via preview; just persist the choice.
        app.theme_picker_original = None;
        execute_slash_action(ctx, host, app, SlashAction::SwitchTheme(theme_id)).await?;
        return Ok(true);
    }
    if composer_slash::lht_picker_active(app.composer.text())
        && app.slash.open
        && let Some(mode) =
            composer_slash::selected_lht_mode(app.composer.text(), app.slash.selected)
    {
        app.composer.clear();
        app.slash.close();
        execute_slash_action(ctx, host, app, SlashAction::SetLhtMode(mode)).await?;
        return Ok(true);
    }
    if composer_slash::model_picker_active(app.composer.text())
        && app.slash.open
        && let Some(model) = composer_slash::selected_model(
            app.composer.text(),
            app.slash.selected,
            &app.model_catalog,
        )
    {
        app.composer.clear();
        app.slash.close();
        execute_slash_action(ctx, host, app, SlashAction::SwitchModel(model)).await?;
        return Ok(true);
    }
    if let Some(action) = composer_slash::try_parse_action(app.composer.text(), &current_ws) {
        app.composer.clear();
        app.slash.close();
        execute_slash_action(ctx, host, app, action).await?;
        return Ok(true);
    }

    if let Some((name, arg)) = composer_slash::split_command_line(app.composer.text())
        && !arg.is_empty()
        && composer_slash::is_workspace_command(name)
    {
        match composer_slash::resolve_workspace_path(arg, &current_ws) {
            Ok(path) => {
                app.composer.clear();
                app.slash.close();
                execute_slash_action(ctx, host, app, SlashAction::SwitchWorkspace(path)).await?;
            }
            Err(err) => {
                app.push_system_line(format!("workspace: {err:#}"));
            }
        }
        return Ok(true);
    }

    if app.slash.open {
        if let Some(cmd) = composer_slash::selected_command(app.composer.text(), app.slash.selected)
        {
            if cmd.takes_arg {
                let arg_empty = composer_slash::split_command_line(app.composer.text())
                    .is_none_or(|(_, arg)| arg.is_empty());
                if arg_empty {
                    composer_slash::apply_palette_selection(&mut app.composer, cmd);
                    app.sync_slash_palette();
                }
            } else {
                app.composer.clear();
                app.slash.close();
                let action = match cmd.action {
                    composer_slash::SlashActionKind::New => SlashAction::NewSession,
                    composer_slash::SlashActionKind::Help => SlashAction::ShowHelp,
                    composer_slash::SlashActionKind::Clear => SlashAction::ClearComposer,
                    composer_slash::SlashActionKind::Workspace
                    | composer_slash::SlashActionKind::Model
                    | composer_slash::SlashActionKind::Lht
                    | composer_slash::SlashActionKind::Theme => {
                        return Ok(true);
                    }
                };
                execute_slash_action(ctx, host, app, action).await?;
            }
        }
        return Ok(true);
    }
    Ok(false)
}

async fn execute_slash_action(
    ctx: &mut crate::cli::context::CliContext,
    host: &mut TuiSessionHost,
    app: &mut AppState,
    action: SlashAction,
) -> Result<()> {
    match action {
        SlashAction::SwitchModel(model) => {
            if app.transcript.is_live_activity() {
                app.push_system_line("model: wait for current turn to finish".to_string());
            } else {
                host.switch_model(model.clone()).await?;
                app.sync_thread_meta(host);
                app.push_system_line(format!("model: {model}"));
            }
        }
        SlashAction::SwitchWorkspace(path) => {
            host.switch_workspace(path.clone()).await?;
            ctx.workspace = path.clone();
            app.reload_after_thread_switch(host).await;
            app.push_system_line(format!(
                "workspace: {}",
                crate::cli::context::display_path(&path)
            ));
        }
        SlashAction::SetLhtMode(mode) => {
            apply_lht_mode_change(app, mode);
        }
        SlashAction::CycleLhtMode => {
            let next = lht_mode::load_lht_composer_mode().cycle();
            apply_lht_mode_change(app, next);
        }
        SlashAction::SwitchTheme(id) => {
            apply_theme_change(app, id);
        }
        SlashAction::CycleTheme => {
            let next = theme::current_id().cycle();
            apply_theme_change(app, next);
        }
        SlashAction::NewSession => {
            host.new_session(ctx).await?;
            app.reload_after_thread_switch(host).await;
            app.push_system_line("new session".to_string());
        }
        SlashAction::ShowHelp => {
            app.show_help = true;
        }
        SlashAction::ClearComposer => {}
    }
    Ok(())
}

fn apply_lht_mode_change(app: &mut AppState, mode: zagens_config::LhtComposerMode) {
    match lht_mode::persist_lht_composer_mode(mode) {
        Ok(()) => {
            app.sync_lht_mode();
            app.push_system_line(format!(
                "lht: {} (applies on next turn)",
                lht_mode::format_lht_mode_label(mode)
            ));
        }
        Err(err) => {
            app.push_system_line(format!("lht: failed to save settings — {err}"));
        }
    }
}

fn apply_theme_change(app: &mut AppState, id: theme::TuiThemeId) {
    theme::install(theme::TuiTheme::resolve(id));
    app.layout.prefs.tui_theme = Some(id.as_str().to_string());
    app.push_system_line(format!("theme: {}", id.label()));
}

/// Called after each Up/Down move while the /theme picker is open.
/// Saves the original theme (first time) and applies the highlighted theme for live preview.
fn preview_theme_selection(app: &mut AppState) {
    if !composer_slash::theme_picker_active(app.composer.text()) || !app.slash.open {
        return;
    }
    // Record the theme that was active before the picker opened (only once).
    if app.theme_picker_original.is_none() {
        app.theme_picker_original = Some(theme::current_id());
    }
    if let Some(id) = composer_slash::selected_theme(app.composer.text(), app.slash.selected) {
        theme::install(theme::TuiTheme::resolve(id));
    }
}

async fn submit_prompt(host: &TuiSessionHost, app: &mut AppState, prompt: &str) {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return;
    }
    if app.transcript.is_live_activity() {
        app.prompt_queue.push_back(prompt.to_string());
        app.push_system_line(format!(
            "Queued message ({}) — sends when the current turn finishes",
            app.prompt_queue.len()
        ));
        return;
    }
    app.prompt_history.push_sent(prompt);
    app.push_user_message(prompt.to_string());
    app.transcript.streaming = true;
    app.blocked_line = None;
    if let Err(err) = host.send_prompt(prompt).await {
        app.transcript.streaming = false;
        app.transcript.close_open_turn();
        app.push_system_line(format!("Failed to start turn: {err:#}"));
    }
}

async fn drain_prompt_queue(host: &TuiSessionHost, app: &mut AppState) {
    while !app.transcript.is_live_activity() {
        let Some(next) = app.prompt_queue.pop_front() else {
            break;
        };
        submit_prompt(host, app, &next).await;
        if app.transcript.is_live_activity() {
            break;
        }
    }
}

fn resolve_mouse_capture(cli: &Cli) -> bool {
    // Mouse capture is ON by default so scroll-wheel works out of the box.
    // Pass --no-mouse-capture to disable (e.g. when copying text with the mouse).
    if cli.no_mouse_capture {
        return false;
    }
    true
}

// ── Mermaid browser-open helpers ──────────────────────────────────────────

/// Build a `mermaid.live` URL that pre-loads the given mermaid source.
///
/// Uses the `#base64:` state format (legacy, no pako required) where the
/// payload is a JSON object `{"code":"...","mermaid":{"theme":"default"}}`.
fn mermaid_live_url(src: &str) -> Option<String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let json = format!(
        r#"{{"code":{code},"mermaid":{{"theme":"default"}}}}"#,
        code = serde_json::to_string(src).ok()?
    );
    let encoded = STANDARD.encode(json.as_bytes());
    Some(format!("https://mermaid.live/edit#base64:{encoded}"))
}

/// Open a URL in the default system browser.
/// Returns an error only if spawning the OS command fails; browser errors are
/// silently ignored because we cannot read them back in a TUI context.
fn open_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}
