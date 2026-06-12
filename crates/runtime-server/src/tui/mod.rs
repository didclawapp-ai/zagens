//! Full-screen terminal UI (`zagens-tui`).

mod activity_strip;
mod app;
mod approval_policy;
mod composer_paste;
mod composer_slash;
mod display_format;
mod draw;
mod focus;
mod harness;
mod inspector;
mod layout;
mod left_rail;
mod markdown_table;
mod overlay;
mod poll;
mod runtime_events;
mod session_host;
mod stderr_log;
mod terminal;
mod theme;
mod transcript;
mod transcript_filter;
mod transcript_turn;

use std::time::Duration;

use anyhow::{Result, bail};
use crossterm::event::{Event, KeyCode, KeyModifiers};

use self::app::AppState;
use self::composer_slash::SlashAction;
use self::focus::FocusRegion;
use self::layout::{InspectorTab, TuiLayoutPrefs};
use self::session_host::TuiSessionHost;
use self::terminal::{TuiTerminal, is_ctrl_c, is_key_press, poll_event};
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
    let mut host = TuiSessionHost::open(&ctx, &cli).await?;
    let resumed = cli.resume.is_some() || cli.continue_session;
    let initial_prompt = cli.prompt.clone();

    let inline_mode = cli.no_alt_screen;
    let mouse_capture = resolve_mouse_capture(&cli);
    let mut app = AppState::new(TuiLayoutPrefs::load(), inline_mode, &host).await;
    if resumed {
        app.seed_resume_banner();
    }
    app.schedule_next_poll();

    let mut tui = TuiTerminal::new(inline_mode, mouse_capture)?;
    let mut ctrl_c_streak = 0u8;
    let mut dirty = true;

    if let Some(prompt) = initial_prompt.filter(|p| !p.trim().is_empty()) {
        submit_prompt(&host, &mut app, &prompt).await?;
        dirty = true;
    }

    loop {
        if dirty {
            tui.terminal.draw(|frame| {
                let area = frame.area();
                app.layout.apply_auto_collapse(area.width);
                let regions = app.layout.regions(area);
                draw::draw(frame, &app, &regions);
            })?;
            dirty = false;
        }

        let poll_sleep = if app.transcript.is_live_activity() {
            Duration::from_millis(120)
        } else if app.poll_due() {
            Duration::from_millis(0)
        } else {
            Duration::from_millis(50)
        };

        tokio::select! {
            runtime_events = host.recv_runtime_events() => {
                if !runtime_events.is_empty() {
                    for event in runtime_events {
                        app.apply_engine_event(event);
                    }
                    dirty = true;
                }
            }
            input = tokio::task::spawn_blocking(move || poll_event(poll_sleep)) => {
                let event = input??;
                if let Some(event) = event {
                    if handle_input_event(
                        &event,
                        &mut ctx,
                        &mut host,
                        &mut app,
                        &mut ctrl_c_streak,
                    ).await? {
                        break;
                    }
                    dirty = true;
                } else if app.transcript.is_live_activity() || app.composer_shows_cursor() {
                    dirty = true;
                } else if app.poll_due() {
                    app.refresh_panels(&host).await;
                    app.refresh_sessions(&host).await;
                    dirty = true;
                }
            }
        }
    }

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
) -> Result<bool> {
    match event {
        Event::Resize(width, _) => {
            app.layout.apply_auto_collapse(*width);
        }
        Event::Paste(text) => {
            app.handle_composer_paste(text);
        }
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
                *ctrl_c_streak = ctrl_c_streak.saturating_add(1);
                if *ctrl_c_streak >= 2 {
                    return Ok(true);
                }
                host.interrupt_turn().await?;
                app.transcript.close_open_turn();
                app.blocked_line = None;
                return Ok(false);
            }
            *ctrl_c_streak = 0;

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
                    app.transcript.toggle_last_tool_expand();
                }
                KeyCode::Char(n @ '1'..='5') if app.layout.focus == FocusRegion::Left => {
                    if let Some(tab) = InspectorTab::from_index(n as u8 - b'0') {
                        app.layout.prefs.set_inspector_tab(tab);
                    }
                }
                KeyCode::Esc if app.layout.focus == FocusRegion::Chat => {
                    if app.composer_focus && app.slash.open {
                        app.composer.clear();
                        app.slash.close();
                    } else {
                        app.composer_focus = !app.composer_focus;
                    }
                }
                KeyCode::Enter if app.layout.focus == FocusRegion::Left => {
                    if let Some(id) = app.sessions.selected_id() {
                        if id != app.thread_id {
                            host.switch_thread(id).await?;
                            app.reload_after_thread_switch(host).await;
                        }
                    }
                }
                KeyCode::Enter if app.layout.focus == FocusRegion::Chat => {
                    if app.composer_focus {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            app.handle_newline();
                        } else if app.slash.open
                            || composer_slash::composer_is_slash_command(&app.composer)
                        {
                            if handle_slash_enter(ctx, host, app).await? {
                                return Ok(false);
                            }
                        } else if app.can_send_prompt() {
                            if let Some(prompt) = app.take_composer_prompt() {
                                submit_prompt(host, app, &prompt).await?;
                            }
                        }
                    } else {
                        app.transcript.toggle_last_tool_expand();
                    }
                }
                KeyCode::Up
                    if app.layout.focus == FocusRegion::Chat
                        && app.composer_focus
                        && app.slash.open =>
                {
                    app.slash.move_up(&app.composer, &app.model_catalog);
                }
                KeyCode::Down
                    if app.layout.focus == FocusRegion::Chat
                        && app.composer_focus
                        && app.slash.open =>
                {
                    app.slash.move_down(&app.composer, &app.model_catalog);
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
    app.layout.focus = if shift {
        app.layout.focus_prev_visible()
    } else {
        app.layout.focus_next_visible()
    };
    if app.layout.focus == FocusRegion::Chat {
        app.composer_focus = !shift;
    }
}

fn handle_j(app: &mut AppState) {
    match app.layout.focus {
        FocusRegion::Left => app.sessions.move_down(),
        FocusRegion::Chat if !app.composer_focus => app.transcript.scroll_up(1),
        _ => {}
    }
}

fn handle_k(app: &mut AppState) {
    match app.layout.focus {
        FocusRegion::Left => app.sessions.move_up(),
        FocusRegion::Chat if !app.composer_focus => app.transcript.scroll_down(1),
        _ => {}
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
            host.auto_approve = true;
            host.thread.auto_approve = true;
            app.sync_thread_meta(host);
            app.clear_approval();
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
    if composer_slash::model_picker_active(&app.composer) && app.slash.open {
        if let Some(model) =
            composer_slash::selected_model(&app.composer, app.slash.selected, &app.model_catalog)
        {
            app.composer.clear();
            app.slash.close();
            execute_slash_action(ctx, host, app, SlashAction::SwitchModel(model)).await?;
            return Ok(true);
        }
    }
    if let Some(action) = composer_slash::try_parse_action(&app.composer, &current_ws) {
        app.composer.clear();
        app.slash.close();
        execute_slash_action(ctx, host, app, action).await?;
        return Ok(true);
    }

    if app.slash.open {
        if let Some(cmd) = composer_slash::selected_command(&app.composer, app.slash.selected) {
            if cmd.takes_arg {
                composer_slash::apply_palette_selection(&mut app.composer, cmd);
                app.sync_slash_palette();
            } else {
                app.composer.clear();
                app.slash.close();
                let action = match cmd.action {
                    composer_slash::SlashActionKind::New => SlashAction::NewSession,
                    composer_slash::SlashActionKind::Help => SlashAction::ShowHelp,
                    composer_slash::SlashActionKind::Clear => SlashAction::ClearComposer,
                    composer_slash::SlashActionKind::Workspace
                    | composer_slash::SlashActionKind::Model => {
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

async fn submit_prompt(host: &TuiSessionHost, app: &mut AppState, prompt: &str) -> Result<()> {
    if app.transcript.is_live_activity() {
        return Ok(());
    }
    app.push_user_message(prompt.to_string());
    app.transcript.streaming = true;
    app.blocked_line = None;
    host.send_prompt(prompt).await?;
    Ok(())
}

fn resolve_mouse_capture(cli: &Cli) -> bool {
    if cli.mouse_capture {
        return true;
    }
    if cli.no_mouse_capture {
        return false;
    }
    !cfg!(windows)
}
