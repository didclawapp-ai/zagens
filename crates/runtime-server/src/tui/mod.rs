//! Full-screen terminal UI (`zagens-tui`).

mod activity_strip;
mod app;
mod approval_policy;
mod automation;
mod composer_editor;
mod composer_paste;
mod composer_paste_batch;
mod composer_paste_guard;
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
use self::app::ComposerEnterAction;
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

    // Channel for RunShell automation actions: output arrives here from background
    // threads and is drained in the biased select! branch without blocking the loop.
    let (shell_tx, mut shell_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Fire SessionStart rules after TUI is up so push_system_line() is visible.
    if !resumed {
        let fired = app.automation_engine.notify_session_start();
        let ctx = automation::EventContext {
            session_id: host.thread_id().to_string(),
            ..Default::default()
        };
        for result in fired {
            for action in result.actions {
                fire_automation_action(&host, &mut app, action, &ctx, &shell_tx).await;
            }
        }
        dirty = true;
    }

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

        // `biased;` guarantees branches are polled in declaration order.
        // Input is first — this ensures keyboard events (Tab, Esc, …) are processed
        // immediately even when the model is streaming heavily.
        tokio::select! {
            biased;

            // 1. Keyboard / mouse input — highest priority.
            maybe_event = input.recv() => {
                if let Some(event) = maybe_event {
                    let mut batch = vec![event];
                    while let Some(extra) = input.try_recv() {
                        batch.push(extra);
                    }
                    let mut quit = false;
                    if let Some(paste) = composer_paste_batch::try_coalesce_terminal_paste(&batch) {
                        if app.layout.focus == FocusRegion::Chat {
                            if !app.composer_focus {
                                app.composer_focus = true;
                            }
                            app.handle_composer_paste(&paste);
                        }
                    } else {
                        for event in batch {
                            if handle_input_event(
                                &event,
                                &mut ctx,
                                &mut host,
                                &mut app,
                                &mut ctrl_c_streak,
                                &mut ctrl_c_last,
                                &shell_tx,
                            ).await? {
                                quit = true;
                                break;
                            }
                        }
                    }
                    if quit {
                        break;
                    }
                    dirty = true;
                }
            }
            // 2. RunShell output arriving from background tasks.
            Some(msg) = shell_rx.recv() => {
                app.push_system_line(msg);
                dirty = true;
            }
            // 3. Streaming / runtime events.
            delta = host.recv_runtime_ui_delta() => {
                let was_streaming = app.transcript.streaming;
                let had_events = !delta.events.is_empty();
                let had_checklist = delta.checklist.is_some() || delta.task_graph.is_some();
                let mut hook_results: Vec<automation::FiredResult> = Vec::new();
                for event in delta.events {
                    hook_results.extend(app.automation_engine.check_engine_event(&event));
                    app.apply_engine_event(event);
                }
                let session_id = app.thread_id.clone();
                for mut fired in hook_results {
                    fired.ctx.session_id = session_id.clone();
                    for action in fired.actions {
                        fire_automation_action(&host, &mut app, action, &fired.ctx, &shell_tx).await;
                    }
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
                // After processing a potentially large delta batch (e.g. thinking tokens),
                // drain all buffered keyboard events so the user never experiences a
                // noticeable input delay even when thinking produces large token batches.
                let mut drain_quit = false;
                loop {
                    match input.try_recv() {
                        Some(event) => {
                            match handle_input_event(
                                &event,
                                &mut ctx,
                                &mut host,
                                &mut app,
                                &mut ctrl_c_streak,
                                &mut ctrl_c_last,
                                &shell_tx,
                            ).await {
                                Ok(true) => { drain_quit = true; break; }
                                Ok(false) => { dirty = true; }
                                Err(e) => return Err(e),
                            }
                        }
                        None => break,
                    }
                }
                if drain_quit { break; }
            }
            // 4. Animation tick (streaming indicator / cursor blink).
            _ = anim_tick.tick(), if app.transcript.is_live_activity() || app.composer_shows_cursor() => {
                dirty = true;
            }
            // 5. Background poll (sessions / panels refresh).
            _ = poll_wake.tick() => {
                if app.poll_due() {
                    app.refresh_panels(&host).await;
                    app.refresh_sessions(&host).await;
                    dirty = true;
                }
            }
            // 6. Automation timer triggers — lowest priority.
            _ = tokio::time::sleep_until(app.automation_engine.next_wake_tokio()) => {
                let fired = app.automation_engine.poll_due();
                if !fired.is_empty() {
                    let session_id = app.thread_id.clone();
                    for mut result in fired {
                        result.ctx.session_id = session_id.clone();
                        for action in result.actions {
                            fire_automation_action(&host, &mut app, action, &result.ctx, &shell_tx).await;
                        }
                    }
                    dirty = true;
                }
            }
        }

        // ── Editor request ─────────────────────────────────────────────────
        // Pause the TUI, open $EDITOR / notepad, reload config, then resume.
        if let Some(path) = app.editor_request.take() {
            tui.shutdown()?;
            match open_in_editor(&path) {
                Ok(()) => {}
                Err(err) => {
                    eprintln!("[automation] editor error: {err}");
                }
            }
            let new_config = automation::AutomationConfig::load();
            app.automation_engine.update_config(new_config);
            tui = TuiTerminal::new(inline_mode, mouse_capture)?;
            terminal::sync_terminal_geometry(&mut tui.terminal, &mut app.layout)?;
            app.terminal_resized = true;
            dirty = true;
            app.push_system_line("automation: config reloaded from disk".to_string());
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
    shell_tx: &tokio::sync::mpsc::UnboundedSender<String>,
) -> Result<bool> {
    match event {
        Event::Resize(width, _) => {
            app.layout.last_terminal_width = *width;
            app.layout.apply_auto_collapse(*width);
            app.terminal_resized = true;
        }
        Event::Paste(text) => {
            if app.layout.focus == FocusRegion::Chat {
                if !app.composer_focus {
                    app.composer_focus = true;
                }
                app.handle_composer_paste(text);
            }
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
            // Keep idle-trigger timers fresh on every keystroke.
            app.automation_engine.notify_activity();
            if app.show_help && key.code == KeyCode::Char('?') {
                app.show_help = false;
                return Ok(false);
            }
            if app.show_help {
                app.show_help = false;
                return Ok(false);
            }

            // Automation overlay — intercept all keys while open.
            if app.show_automation {
                return handle_automation_key(*key, host, app).await;
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
                KeyCode::Char('?') if composer_absorbs_text_keys(app) => {
                    app.handle_char('?');
                }
                KeyCode::Char('?') => {
                    app.show_help = !app.show_help;
                }
                KeyCode::Tab if !app.approval_open() && composer_absorbs_text_keys(app) => {
                    app.handle_char('\t');
                }
                KeyCode::Tab if !app.approval_open() => {
                    handle_tab_focus(app, key.modifiers.contains(KeyModifiers::SHIFT));
                }
                KeyCode::Char('[') if composer_absorbs_text_keys(app) => {
                    app.handle_char('[');
                }
                KeyCode::Char('[') => app.layout.toggle_left(),
                KeyCode::Char(']') if composer_absorbs_text_keys(app) => {
                    app.handle_char(']');
                }
                KeyCode::Char(']') => app.layout.toggle_right(),
                KeyCode::Char('n')
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && app.layout.focus == FocusRegion::Left =>
                {
                    host.new_session(ctx).await?;
                    app.reload_after_thread_switch(host).await;
                }
                KeyCode::Char('j')
                    if app.layout.focus == FocusRegion::Chat && composer_absorbs_text_keys(app) =>
                {
                    app.handle_char('j');
                }
                KeyCode::Char('k')
                    if app.layout.focus == FocusRegion::Chat && composer_absorbs_text_keys(app) =>
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
                        match app.handle_composer_enter(key.modifiers.contains(KeyModifiers::SHIFT))
                        {
                            ComposerEnterAction::Slash => {
                                if handle_slash_enter(ctx, host, app, shell_tx).await? {
                                    return Ok(false);
                                }
                            }
                            ComposerEnterAction::Send => {
                                if let Some(prompt) = app.take_composer_prompt() {
                                    submit_prompt(host, app, &prompt).await;
                                }
                            }
                            ComposerEnterAction::Newline | ComposerEnterAction::None => {}
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
                KeyCode::Char('v') | KeyCode::Char('V')
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && app.layout.focus == FocusRegion::Chat
                        && app.composer_focus =>
                {
                    app.paste_from_clipboard();
                }
                KeyCode::Insert
                    if key.modifiers.contains(KeyModifiers::SHIFT)
                        && app.layout.focus == FocusRegion::Chat
                        && app.composer_focus =>
                {
                    app.paste_from_clipboard();
                }
                KeyCode::Backspace => app.handle_backspace(),
                KeyCode::Delete if app.layout.focus == FocusRegion::Chat && app.composer_focus => {
                    app.composer.delete_forward();
                    app.sync_slash_palette();
                }
                KeyCode::Char(ch) => {
                    if app.composer_paste_guard.paste_active()
                        && app.layout.focus == FocusRegion::Chat
                    {
                        app.composer_focus = true;
                    }
                    app.handle_char(ch);
                }
                _ => {}
            }
        }
        _ => {}
    }
    Ok(false)
}

fn composer_absorbs_text_keys(app: &AppState) -> bool {
    (app.layout.focus == FocusRegion::Chat && app.composer_focus)
        || app.composer_paste_guard.paste_active()
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

// ── Automation overlay key handler ─────────────────────────────────────────

async fn handle_automation_key(
    key: crossterm::event::KeyEvent,
    host: &mut TuiSessionHost,
    app: &mut AppState,
) -> Result<bool> {
    let _ = host;

    if app.automation_ui.edit_mode {
        // ── Edit mode ──────────────────────────────────────────────────────
        match key.code {
            KeyCode::Esc => {
                app.automation_ui.cancel_edit();
            }
            KeyCode::Enter => {
                save_automation_edit(app);
            }
            KeyCode::Tab => {
                app.automation_ui.next_field();
            }
            KeyCode::BackTab => {
                app.automation_ui.prev_field();
            }
            KeyCode::Left => {
                if !app.automation_ui.edit_field.is_text_input() {
                    app.automation_ui.cycle_selector(false);
                }
            }
            KeyCode::Right => {
                if !app.automation_ui.edit_field.is_text_input() {
                    app.automation_ui.cycle_selector(true);
                }
            }
            KeyCode::Up => {
                if !app.automation_ui.edit_field.is_text_input() {
                    app.automation_ui.cycle_selector(false);
                }
            }
            KeyCode::Down => {
                if !app.automation_ui.edit_field.is_text_input() {
                    app.automation_ui.cycle_selector(true);
                }
            }
            KeyCode::Backspace => {
                app.automation_ui.backspace();
            }
            KeyCode::Char(ch) => {
                app.automation_ui.input_char(ch);
            }
            _ => {}
        }
    } else {
        // ── List mode ──────────────────────────────────────────────────────
        let len = app.automation_engine.config.rules.len();
        match key.code {
            KeyCode::Esc => {
                app.show_automation = false;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                app.automation_ui.move_down(len);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.automation_ui.move_up();
            }
            KeyCode::Char(' ') => {
                toggle_automation_rule(app);
            }
            KeyCode::Char('n') => {
                app.automation_ui.start_new();
            }
            KeyCode::Enter => {
                let idx = app.automation_ui.list_selected;
                if let Some(rule) = app.automation_engine.config.rules.get(idx) {
                    let rule = rule.clone();
                    app.automation_ui.start_edit(&rule);
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                delete_automation_rule(app);
            }
            // Open the TOML config file in $EDITOR / notepad.
            KeyCode::Char('e') => {
                if let Some(path) = automation::AutomationConfig::path() {
                    // Ensure the file exists before opening.
                    if !path.exists() {
                        let _ = automation::AutomationConfig::default().save();
                    }
                    app.editor_request = Some(path);
                    app.show_automation = false;
                } else {
                    app.push_system_line("automation: could not resolve config path".to_string());
                }
            }
            _ => {}
        }
    }
    Ok(false)
}

fn save_automation_edit(app: &mut AppState) {
    let editing_id = app.automation_ui.editing_id;
    let built_rule_for_new;
    let built_rule_for_update;

    if let Some(id) = editing_id {
        built_rule_for_update = app.automation_ui.build_rule(id);
        built_rule_for_new = None;
    } else {
        let id = app.automation_engine.config.alloc_id();
        built_rule_for_new = app.automation_ui.build_rule(id);
        built_rule_for_update = None;
    }

    if let Some(id) = editing_id {
        if let Some(rule) = built_rule_for_update {
            if let Some(existing) = app
                .automation_engine
                .config
                .rules
                .iter_mut()
                .find(|r| r.id == id)
            {
                existing.name = rule.name;
                existing.trigger = rule.trigger;
                // Preserve extra actions the user may have added via direct TOML
                // editing; only overwrite the first (primary) action.
                if !rule.actions.is_empty() {
                    if existing.actions.is_empty() {
                        existing.actions = rule.actions;
                    } else {
                        existing.actions[0] = rule.actions.into_iter().next().unwrap();
                    }
                }
            }
        }
    } else if let Some(rule) = built_rule_for_new {
        app.automation_engine.config.rules.push(rule);
    }

    app.automation_engine.save();
    app.automation_engine
        .reset_timers(std::time::Instant::now());
    app.automation_ui.edit_mode = false;
    let len = app.automation_engine.config.rules.len();
    app.automation_ui.clamp_selection(len);
}

fn toggle_automation_rule(app: &mut AppState) {
    let idx = app.automation_ui.list_selected;
    if let Some(rule) = app.automation_engine.config.rules.get_mut(idx) {
        rule.enabled = !rule.enabled;
    }
    app.automation_engine.save();
    app.automation_engine
        .reset_timers(std::time::Instant::now());
}

fn delete_automation_rule(app: &mut AppState) {
    let idx = app.automation_ui.list_selected;
    if idx < app.automation_engine.config.rules.len() {
        app.automation_engine.config.rules.remove(idx);
    }
    app.automation_engine.save();
    app.automation_engine
        .reset_timers(std::time::Instant::now());
    let len = app.automation_engine.config.rules.len();
    app.automation_ui.clamp_selection(len);
}

/// Fire a single automation action with an optional event context for template substitution.
async fn fire_automation_action(
    host: &TuiSessionHost,
    app: &mut AppState,
    action: automation::ActionKind,
    ctx: &automation::EventContext,
    shell_tx: &tokio::sync::mpsc::UnboundedSender<String>,
) {
    match action {
        automation::ActionKind::SendPrompt { text } => {
            let resolved = ctx.apply(&text);
            submit_prompt(host, app, &resolved).await;
        }
        automation::ActionKind::SlashRun { cmd } => {
            let resolved = ctx.apply(&cmd);
            let with_slash = if resolved.starts_with('/') {
                resolved.clone()
            } else {
                format!("/{resolved}")
            };
            let ws = app.workspace.clone();
            if let Some(slash_action) = composer_slash::try_parse_action(&with_slash, &ws) {
                app.push_system_line(format!("[auto] {with_slash}"));
                let _ = execute_slash_noop(app, slash_action);
            } else {
                app.push_system_line(format!("[auto] slash '{with_slash}' — not dispatched (needs host context, use SendPrompt instead)"));
            }
        }
        automation::ActionKind::RunShell { cmd } => {
            let resolved = ctx.apply(&cmd);
            app.push_system_line(format!("[auto] $ {resolved}"));
            // Fire-and-forget: run in a thread-pool thread, send output back
            // via the shell_tx channel without blocking the TUI event loop.
            let tool_name_s = ctx.tool_name.clone().unwrap_or_default();
            let session_id_s = ctx.session_id.clone();
            let error_msg_s = ctx.error_message.clone().unwrap_or_default();
            let tx = shell_tx.clone();
            tokio::task::spawn_blocking(move || {
                match run_shell_cmd_sync(&resolved, &tool_name_s, &session_id_s, &error_msg_s) {
                    Ok(out) if !out.is_empty() => {
                        let _ = tx.send(format!("[auto output] {out}"));
                    }
                    Err(err) => {
                        let _ = tx.send(format!("[auto error] {err}"));
                    }
                    _ => {}
                }
            });
            // Note: we do NOT await the handle — the task runs in the background.
        }
        automation::ActionKind::Notify { message } => {
            let resolved = ctx.apply(&message);
            app.push_system_line(format!("[notify] {resolved}"));
        }
    }
}

/// Dispatch slash actions that do not require live `host`/`ctx` access.
/// Returns `true` if the action was handled, `false` if it needs host context.
fn execute_slash_noop(app: &mut AppState, action: SlashAction) -> bool {
    match action {
        SlashAction::ShowHelp => {
            app.show_help = true;
            true
        }
        SlashAction::ShowAutomation => {
            app.show_automation = true;
            app.automation_ui.edit_mode = false;
            let len = app.automation_engine.config.rules.len();
            app.automation_ui.clamp_selection(len);
            true
        }
        SlashAction::SwitchTheme(id) => {
            apply_theme_change(app, id);
            true
        }
        SlashAction::CycleTheme => {
            let next = theme::current_id().cycle();
            apply_theme_change(app, next);
            true
        }
        SlashAction::SetLhtMode(mode) => {
            apply_lht_mode_change(app, mode);
            true
        }
        SlashAction::CycleLhtMode => {
            let next = lht_mode::load_lht_composer_mode().cycle();
            apply_lht_mode_change(app, next);
            true
        }
        SlashAction::ClearComposer => {
            app.composer.clear();
            true
        }
        // These need host context — caller should use SendPrompt or RunShell instead.
        SlashAction::NewSession | SlashAction::SwitchModel(_) | SlashAction::SwitchWorkspace(_) => {
            false
        }
    }
}

/// Spawn a shell command in a blocking context and return trimmed output (≤ 500 chars).
/// Has an implicit OS timeout via the process itself; callers should wrap in
/// `tokio::task::spawn_blocking` + `tokio::time::timeout` for hard bounds.
fn run_shell_cmd_sync(
    cmd: &str,
    tool_name: &str,
    session_id: &str,
    error_msg: &str,
) -> std::io::Result<String> {
    #[cfg(target_os = "windows")]
    let child = std::process::Command::new("cmd")
        .args(["/c", cmd])
        .env("ZAGENS_TOOL_NAME", tool_name)
        .env("ZAGENS_SESSION_ID", session_id)
        .env("ZAGENS_ERROR_MSG", error_msg)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    #[cfg(not(target_os = "windows"))]
    let child = std::process::Command::new("sh")
        .args(["-c", cmd])
        .env("ZAGENS_TOOL_NAME", tool_name)
        .env("ZAGENS_SESSION_ID", session_id)
        .env("ZAGENS_ERROR_MSG", error_msg)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let output = child.wait_with_output()?;
    let raw = if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    let text = String::from_utf8_lossy(&raw);
    let trimmed = text.trim();
    if trimmed.len() > 500 {
        Ok(format!("{}…", &trimmed[..500]))
    } else {
        Ok(trimmed.to_string())
    }
}

/// Open a file path in the user's `$EDITOR` (Unix) or `notepad` / `$EDITOR` (Windows)
/// and block until the editor exits.
fn open_in_editor(path: &std::path::Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "notepad".to_string());
    #[cfg(not(target_os = "windows"))]
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    std::process::Command::new(&editor)
        .arg(path)
        .status()
        .map_err(|e| anyhow::anyhow!("failed to launch editor '{editor}': {e}"))?;
    Ok(())
}

async fn handle_slash_enter(
    ctx: &mut crate::cli::context::CliContext,
    host: &mut TuiSessionHost,
    app: &mut AppState,
    shell_tx: &tokio::sync::mpsc::UnboundedSender<String>,
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
        execute_slash_action(ctx, host, app, SlashAction::SwitchTheme(theme_id), shell_tx).await?;
        return Ok(true);
    }
    if composer_slash::lht_picker_active(app.composer.text())
        && app.slash.open
        && let Some(mode) =
            composer_slash::selected_lht_mode(app.composer.text(), app.slash.selected)
    {
        app.composer.clear();
        app.slash.close();
        execute_slash_action(ctx, host, app, SlashAction::SetLhtMode(mode), shell_tx).await?;
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
        execute_slash_action(ctx, host, app, SlashAction::SwitchModel(model), shell_tx).await?;
        return Ok(true);
    }
    if let Some(action) = composer_slash::try_parse_action(app.composer.text(), &current_ws) {
        app.composer.clear();
        app.slash.close();
        execute_slash_action(ctx, host, app, action, shell_tx).await?;
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
                execute_slash_action(ctx, host, app, SlashAction::SwitchWorkspace(path), shell_tx)
                    .await?;
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
                    composer_slash::SlashActionKind::Automation => SlashAction::ShowAutomation,
                    composer_slash::SlashActionKind::Clear => SlashAction::ClearComposer,
                    composer_slash::SlashActionKind::Workspace
                    | composer_slash::SlashActionKind::Model
                    | composer_slash::SlashActionKind::Lht
                    | composer_slash::SlashActionKind::Theme => {
                        return Ok(true);
                    }
                };
                execute_slash_action(ctx, host, app, action, shell_tx).await?;
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
    shell_tx: &tokio::sync::mpsc::UnboundedSender<String>,
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
            let fired = app.automation_engine.notify_session_start();
            let session_id = app.thread_id.clone();
            let event_ctx = automation::EventContext {
                session_id,
                ..Default::default()
            };
            for result in fired {
                for action in result.actions {
                    fire_automation_action(&host, &mut *app, action, &event_ctx, shell_tx).await;
                }
            }
            app.push_system_line("new session".to_string());
        }
        SlashAction::ShowHelp => {
            app.show_help = true;
        }
        SlashAction::ShowAutomation => {
            app.show_automation = true;
            app.automation_ui.edit_mode = false;
            let len = app.automation_engine.config.rules.len();
            app.automation_ui.clamp_selection(len);
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
