//! TUI application state (Phase 2).

use std::path::PathBuf;
use std::time::Instant;

use super::composer_paste::{normalize_paste_text, read_clipboard_text};
use super::composer_slash::{SlashCommandState, composer_is_slash_command, render_palette};
use super::display_format::{
    composer_cursor_blink_on, display_width, pad_line_display_width, truncate_display_width,
};
use super::theme::{self, COMPOSER_PROMPT};

use super::focus::{FocusRegion, RightSubfocus};
use super::harness::{ChecklistSnapshot, blocked_suffix};
use super::inspector::{
    AgentEntry, InspectorCache, InspectorInteraction, LhtPaneUi, agents_line_count, git_diff_patch,
    lht_line_count, read_text_preview,
};
use super::layout::{InspectorTab, LayoutEngine, TuiLayoutPrefs};
use super::left_rail::SessionList;
use super::lht_mode::{format_lht_mode_label, load_lht_composer_mode};
use super::overlay::PendingApproval;
use super::poll::poll_interval;
use super::session_host::TuiSessionHost;
use super::task_graph::{TaskGraphSnapshot, title_bar_harness_line_from_graph};
use super::transcript::{TranscriptItem, TranscriptState, apply_event};
use crate::core::events::Event;

pub struct AppState {
    pub layout: LayoutEngine,
    pub transcript: TranscriptState,
    pub composer: String,
    pub composer_focus: bool,
    pub thread_id: String,
    pub workspace_display: String,
    pub workspace: PathBuf,
    pub model_display: String,
    pub model_catalog: Vec<String>,
    pub run_mode_display: String,
    pub task_type_display: String,
    pub lht_mode_display: String,
    pub approval_display: String,
    pub approval_toggle_enabled: bool,
    pub harness_line: String,
    pub blocked_line: Option<String>,
    pub context_pct: Option<u8>,
    pub sessions: SessionList,
    pub inspector: InspectorCache,
    pub inspector_ui: InspectorInteraction,
    pub task_graph: Option<TaskGraphSnapshot>,
    pub lht_pane_expanded: bool,
    pub lht_auto_opened: bool,
    pub lht_ui: LhtPaneUi,
    pub right_subfocus: RightSubfocus,
    pub right_panel_height: usize,
    pub right_inspector_height: usize,
    pub right_lht_height: usize,
    pub agents: Vec<AgentEntry>,
    pub pending_approval: Option<PendingApproval>,
    pub show_help: bool,
    /// Set on terminal resize; next draw clears the backing buffer (shrunk-area ghosts).
    pub terminal_resized: bool,
    pub cursor_blink_since: Instant,
    next_poll: Instant,
    pub slash: SlashCommandState,
}

impl AppState {
    pub async fn new(
        layout_prefs: TuiLayoutPrefs,
        inline_mode: bool,
        host: &TuiSessionHost,
    ) -> Self {
        let mut transcript = TranscriptState::default();
        if let Ok(history) = host.load_history() {
            transcript.items = history;
        }
        let mut inspector = InspectorCache::default();
        inspector.refresh_static(&host.thread.workspace, host.config());
        let threads = host.list_workspace_threads().await.unwrap_or_default();
        let sessions = SessionList::from_threads(threads, host.thread_id());

        let mut state = Self {
            layout: LayoutEngine::new(inline_mode, layout_prefs),
            transcript,
            composer: String::new(),
            composer_focus: true,
            thread_id: host.thread_id().to_string(),
            workspace_display: host.workspace_display(),
            workspace: host.thread.workspace.clone(),
            model_display: String::new(),
            model_catalog: Vec::new(),
            run_mode_display: String::new(),
            task_type_display: String::new(),
            lht_mode_display: String::new(),
            approval_display: String::new(),
            approval_toggle_enabled: false,
            harness_line: title_bar_harness_line_from_graph(None),
            blocked_line: None,
            context_pct: None,
            sessions,
            inspector,
            inspector_ui: InspectorInteraction::default(),
            task_graph: None,
            lht_pane_expanded: false,
            lht_auto_opened: false,
            lht_ui: LhtPaneUi::default(),
            right_subfocus: RightSubfocus::Inspector,
            right_panel_height: 20,
            right_inspector_height: 20,
            right_lht_height: 0,
            agents: Vec::new(),
            pending_approval: None,
            show_help: false,
            terminal_resized: false,
            cursor_blink_since: Instant::now(),
            next_poll: Instant::now(),
            slash: SlashCommandState::default(),
        };
        state.sync_thread_meta(host);
        state.sync_lht_mode();
        if let Some(graph) = host.fetch_task_graph().await {
            state.apply_task_graph_snapshot(graph);
        } else if let Some(checklist) = host.fetch_checklist() {
            state.merge_checklist_snapshot(checklist);
        }
        state
    }

    pub fn lht_pane_visible(&self) -> bool {
        self.lht_pane_expanded && self.task_graph.as_ref().is_some_and(|g| g.has_activity())
    }

    pub fn toggle_lht_pane(&mut self) {
        if !self.task_graph.as_ref().is_some_and(|g| g.has_activity()) {
            return;
        }
        self.lht_pane_expanded = !self.lht_pane_expanded;
        if self.lht_pane_expanded {
            self.right_subfocus = RightSubfocus::Lht;
            self.lht_ui.reset();
        }
    }

    pub fn focus_inspector_upper(&mut self) {
        self.right_subfocus = RightSubfocus::Inspector;
    }

    pub fn lht_scroll_up(&mut self) {
        self.lht_ui.scroll_up(1);
    }

    pub fn lht_scroll_down(&mut self) {
        let total = lht_line_count(self.task_graph.as_ref());
        let visible = self.right_lht_height.max(4);
        let max_scroll = total.saturating_sub(visible);
        self.lht_ui.scroll_down(max_scroll, 1);
    }

    pub fn right_rail_scroll_up(&mut self, workspace: &std::path::Path) {
        if self.lht_pane_visible() && self.right_subfocus == RightSubfocus::Lht {
            self.lht_scroll_up();
        } else {
            self.inspector_scroll_up(workspace);
        }
    }

    pub fn right_rail_scroll_down(&mut self, workspace: &std::path::Path) {
        if self.lht_pane_visible() && self.right_subfocus == RightSubfocus::Lht {
            self.lht_scroll_down();
        } else {
            self.inspector_scroll_down(workspace);
        }
    }

    pub fn apply_task_graph_snapshot(&mut self, graph: TaskGraphSnapshot) {
        if graph.has_activity() {
            if !self.lht_auto_opened {
                self.lht_pane_expanded = true;
                self.lht_auto_opened = true;
            }
        } else {
            self.lht_pane_expanded = false;
        }
        self.harness_line = title_bar_harness_line_from_graph(Some(&graph));
        self.task_graph = Some(graph);
    }

    pub fn merge_checklist_snapshot(&mut self, checklist: ChecklistSnapshot) {
        if let Some(graph) = &mut self.task_graph {
            graph.merge_checklist(checklist);
            let graph = graph.clone();
            self.apply_task_graph_snapshot(graph);
        } else {
            self.apply_task_graph_snapshot(TaskGraphSnapshot::from_checklist(checklist));
        }
    }

    pub fn refresh_task_graph_from_host(&mut self, host: &TuiSessionHost) {
        if let Some(checklist) = host.fetch_checklist() {
            self.merge_checklist_snapshot(checklist);
        }
    }

    pub fn inspector_panel_height(&self) -> usize {
        if self.lht_pane_visible() {
            self.right_inspector_height
        } else {
            self.right_panel_height
        }
    }

    pub fn switch_inspector_tab(&mut self, tab: InspectorTab) {
        if self.layout.prefs.inspector_tab() != tab {
            self.inspector_ui.reset_for_tab();
        }
        self.layout.prefs.set_inspector_tab(tab);
    }

    pub fn inspector_scroll_up(&mut self, _workspace: &std::path::Path) {
        if self.inspector_ui.in_detail_view() {
            self.inspector_ui.scroll_up(1);
            return;
        }
        let tab = self.layout.prefs.inspector_tab();
        let visible = self.inspector_panel_height().max(4);
        match tab {
            InspectorTab::Files => self.inspector_ui.file_move_up(),
            InspectorTab::Diff => self.inspector_ui.diff_move_up(),
            InspectorTab::Agents => self.inspector_ui.agents_move_up(),
            InspectorTab::Mcp => self.inspector_ui.mcp_move_up(),
        }
        let _ = visible;
    }

    pub fn inspector_scroll_down(&mut self, workspace: &std::path::Path) {
        let visible = self.inspector_panel_height().max(4);
        if self.inspector_ui.in_detail_view() {
            let total = self.inspector_detail_line_count(workspace);
            let max_scroll = total.saturating_sub(visible);
            self.inspector_ui.scroll_down(max_scroll, 1);
            return;
        }
        let tab = self.layout.prefs.inspector_tab();
        match tab {
            InspectorTab::Files => {
                let count = self.inspector.file_tree.line_count();
                self.inspector_ui.file_move_down(count, visible);
            }
            InspectorTab::Diff => {
                let count = self.inspector.diff.line_count();
                self.inspector_ui.diff_move_down(count, visible);
            }
            InspectorTab::Agents => {
                let count = agents_line_count(&self.inspector.agents);
                self.inspector_ui.agents_move_down(count, visible);
            }
            InspectorTab::Mcp => {
                let count = self.inspector.mcp.servers.len().max(1);
                self.inspector_ui.mcp_move_down(count, visible);
            }
        }
    }

    fn inspector_detail_line_count(&self, workspace: &std::path::Path) -> usize {
        if let Some(rel) = self.inspector_ui.file_preview_rel.as_deref() {
            return read_text_preview(workspace, rel).len() + 1;
        }
        if let Some(path) = self.inspector_ui.diff_detail_path.as_deref() {
            return git_diff_patch(workspace, self.inspector_ui.diff_staged, path, 400).len() + 1;
        }
        0
    }

    pub fn inspector_activate(&mut self, workspace: &std::path::Path) {
        if self.inspector_ui.in_detail_view() {
            return;
        }
        let tab = self.layout.prefs.inspector_tab();
        match tab {
            InspectorTab::Files => {
                let cursor = self.inspector_ui.file_cursor;
                let Some(line) = self.inspector.file_tree.line_at(cursor) else {
                    return;
                };
                if line.is_dir {
                    self.inspector.file_tree.toggle_cursor(cursor, workspace);
                    self.inspector_ui
                        .clamp_file_cursor(self.inspector.file_tree.line_count());
                } else {
                    self.inspector_ui.file_preview_rel = Some(line.rel.clone());
                    self.inspector_ui.scroll = 0;
                }
            }
            InspectorTab::Diff => {
                if self.inspector.diff.entries.is_empty() {
                    return;
                }
                if let Some(path) = self
                    .inspector
                    .diff
                    .entry_path(self.inspector_ui.diff_cursor)
                {
                    self.inspector_ui.diff_detail_path = Some(path.to_string());
                    self.inspector_ui.scroll = 0;
                }
            }
            InspectorTab::Mcp => {
                let Some(server) = self.inspector.mcp.servers.get(self.inspector_ui.mcp_cursor)
                else {
                    return;
                };
                if self.inspector_ui.mcp_expanded.as_deref() == Some(server.name.as_str()) {
                    self.inspector_ui.mcp_expanded = None;
                } else {
                    self.inspector_ui.mcp_expanded = Some(server.name.clone());
                }
                self.inspector_ui.scroll = 0;
            }
            InspectorTab::Agents => {}
        }
    }

    pub fn inspector_back(&mut self) {
        if self.inspector_ui.in_detail_view() {
            self.inspector_ui.clear_detail_views();
        } else if self.inspector_ui.mcp_expanded.is_some() {
            self.inspector_ui.mcp_expanded = None;
            self.inspector_ui.scroll = 0;
        }
    }

    pub fn toggle_inspector_file_entry(&mut self, workspace: &std::path::Path) {
        self.inspector_activate(workspace);
    }

    pub fn toggle_diff_staged(&mut self, workspace: &std::path::Path) {
        self.inspector_ui.diff_staged = !self.inspector_ui.diff_staged;
        self.inspector_ui.diff_detail_path = None;
        self.inspector
            .refresh_diff_only(workspace, self.inspector_ui.diff_staged);
        self.inspector_ui.scroll = 0;
        self.inspector_ui
            .clamp_diff_cursor(self.inspector.diff.line_count());
    }

    pub fn refresh_workspace_inspector(&mut self, host: &TuiSessionHost) {
        self.inspector.refresh_files_diff(
            &host.thread.workspace,
            host.config(),
            self.inspector_ui.diff_staged,
        );
        self.inspector_ui
            .clamp_file_cursor(self.inspector.file_tree.line_count());
        self.inspector_ui
            .clamp_diff_cursor(self.inspector.diff.line_count());
        self.inspector_ui
            .clamp_agents_cursor(agents_line_count(&self.inspector.agents));
        self.inspector_ui
            .clamp_mcp_cursor(self.inspector.mcp.servers.len().max(1));
    }

    pub fn sync_thread_meta(&mut self, host: &TuiSessionHost) {
        self.model_display = host.thread.model.clone();
        self.model_catalog = host.model_catalog();
        self.run_mode_display = format_run_mode_label(&host.thread.mode, host.yolo);
        self.task_type_display = format_task_type_label(&host.thread.task_type);
        self.workspace_display = host.workspace_display();
        self.workspace = host.thread.workspace.clone();
        let (approval, toggle) = host.approval_footer_meta();
        self.approval_display = approval;
        self.approval_toggle_enabled = toggle;
    }

    pub fn sync_lht_mode(&mut self) {
        self.lht_mode_display = format_lht_mode_label(load_lht_composer_mode());
    }

    pub fn current_lht_composer_mode(&self) -> zagens_config::LhtComposerMode {
        load_lht_composer_mode()
    }

    pub fn title_status_line(&self) -> String {
        let mut parts = vec![self.thread_id.clone(), self.harness_line.clone()];
        if let Some(pct) = self.context_pct {
            parts.push(format!("ctx {pct}%"));
        }
        if let Some(blocked) = &self.blocked_line {
            parts.push(blocked.clone());
        }
        parts.join(" · ")
    }

    pub fn apply_engine_event(&mut self, event: Event) {
        if let Event::ApprovalRequired {
            id,
            tool_name,
            description,
            approval_key,
        } = &event
        {
            self.pending_approval = Some(PendingApproval {
                id: id.clone(),
                tool_name: tool_name.clone(),
                description: description.clone(),
                approval_key: approval_key.clone(),
            });
        }
        if let Event::AgentSpawned { id, .. } = &event {
            self.agents.push(AgentEntry {
                id: id.clone(),
                status: "spawned".to_string(),
            });
        }
        if let Event::AgentProgress { id, status, .. } = &event {
            if let Some(entry) = self.agents.iter_mut().find(|a| a.id == *id) {
                entry.status = status.clone();
            } else {
                self.agents.push(AgentEntry {
                    id: id.clone(),
                    status: status.clone(),
                });
            }
        }
        if let Event::AgentComplete { id, .. } = &event {
            if let Some(entry) = self.agents.iter_mut().find(|a| a.id == *id) {
                entry.status = "done".to_string();
            }
        }

        apply_event(&mut self.transcript, event);
        self.blocked_line = blocked_suffix(self.transcript.end_reason.as_deref());
        self.inspector.agents = self.agents.clone();
    }

    pub fn clear_approval(&mut self) {
        self.pending_approval = None;
    }

    pub fn approval_open(&self) -> bool {
        self.pending_approval.is_some()
    }

    pub fn push_user_message(&mut self, text: String) {
        self.transcript.begin_turn(text);
    }

    pub fn transcript_render(
        &self,
        max_lines: usize,
        max_cols: usize,
    ) -> Vec<ratatui::text::Line<'static>> {
        self.transcript.render_styled_lines(max_lines, max_cols)
    }

    pub fn composer_shows_cursor(&self) -> bool {
        self.composer_focus
            && self.layout.focus == FocusRegion::Chat
            && !self.approval_open()
            && !self.show_help
            && !self.transcript.is_live_activity()
    }

    pub fn composer_render(
        &self,
        max_lines: usize,
        max_cols: usize,
    ) -> Vec<ratatui::text::Line<'static>> {
        use ratatui::text::{Line, Span};

        let show_cursor = self.composer_shows_cursor();
        let cursor_on = show_cursor && composer_cursor_blink_on(self.cursor_blink_since);

        if self.composer.is_empty() {
            let hint_body = if self.transcript.is_live_activity() {
                " waiting for reply...  Ctrl+C interrupt  Esc scroll  Up/Down history"
            } else {
                " type prompt...  / commands  /model  Ctrl+V paste  Enter send  Shift+Enter newline  Esc scroll"
            };
            let hint_style = if self.composer_focus {
                theme::composer_idle()
            } else {
                theme::hint()
            };
            let body_padded = pad_line_display_width(
                hint_body,
                max_cols.saturating_sub(display_width(COMPOSER_PROMPT)),
            );
            let mut lines = vec![Line::from(vec![
                Span::styled(COMPOSER_PROMPT.to_string(), theme::composer_prompt()),
                Span::styled(body_padded, hint_style),
            ])];
            if show_cursor {
                let caret = if cursor_on {
                    format!("{COMPOSER_PROMPT}-")
                } else {
                    format!("{COMPOSER_PROMPT} ")
                };
                lines.push(theme::composer_line(
                    &pad_line_display_width(&caret, max_cols),
                    self.composer_focus,
                ));
            }
            return lines;
        }

        let wrapped: Vec<String> = self
            .composer
            .lines()
            .flat_map(|line| {
                super::display_format::wrap_display_line(
                    &format!("{COMPOSER_PROMPT}{line}"),
                    max_cols,
                )
            })
            .collect();
        let start = wrapped.len().saturating_sub(max_lines.max(1));
        let visible = &wrapped[start..];
        visible
            .iter()
            .enumerate()
            .map(|(idx, line)| {
                let mut content = line.to_string();
                if show_cursor && idx + 1 == visible.len() && cursor_on {
                    if display_width(&content) + 1 > max_cols {
                        content = truncate_display_width(&content, max_cols.saturating_sub(1));
                    }
                    content.push('-');
                }
                let padded = pad_line_display_width(&content, max_cols);
                theme::composer_line(&padded, self.composer_focus)
            })
            .collect()
    }

    pub fn composer_footer_line(&self, max_cols: usize) -> ratatui::text::Line<'static> {
        use ratatui::text::{Line, Span};

        let chip = |text: String, style: ratatui::style::Style| Span::styled(text, style);
        let sep = Span::styled(" | ", theme::footer_separator());

        let mut spans = vec![
            chip(
                self.model_display.clone(),
                theme::footer_chip(theme::footer_model()),
            ),
            sep.clone(),
            chip(
                self.run_mode_display.clone(),
                theme::footer_chip(theme::footer_mode()),
            ),
            sep.clone(),
            chip(
                self.task_type_display.clone(),
                theme::footer_chip(theme::footer_task()),
            ),
            sep.clone(),
            chip(
                self.lht_mode_display.clone(),
                theme::footer_chip(theme::footer_lht()),
            ),
        ];

        if !self.workspace_display.is_empty() {
            spans.push(sep.clone());
            spans.push(Span::styled(
                truncate_footer_workspace(&self.workspace_display, max_cols / 3),
                theme::footer_workspace(),
            ));
        }

        if let Some(pct) = self.context_pct {
            spans.push(sep.clone());
            spans.push(Span::styled(format!("ctx {pct}%"), theme::footer_context()));
        }

        spans.push(sep.clone());
        let approval_label = if self.approval_toggle_enabled {
            format!("approve: {} ^A", self.approval_display)
        } else {
            format!("approve: {}", self.approval_display)
        };
        spans.push(chip(
            approval_label,
            theme::footer_chip(theme::approval_color(&self.approval_display)),
        ));

        let focus = if self.transcript.is_live_activity() {
            "wait"
        } else if self.composer_focus {
            "edit"
        } else {
            "scroll"
        };
        spans.push(sep);
        spans.push(Span::styled(format!("[{focus}]"), theme::footer_muted()));

        let line = Line::from(spans);
        let plain = line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>();
        let pad_style = theme::shell_main();
        if super::display_format::display_width(&plain) <= max_cols {
            return super::display_format::pad_styled_line(line, max_cols, pad_style);
        }

        let compact = format!(
            "{} · {} · {} · {} · approve:{}",
            self.model_display,
            self.run_mode_display,
            self.task_type_display,
            self.lht_mode_display,
            self.approval_display
        );
        let padded = super::display_format::pad_line_display_width(&compact, max_cols);
        Line::from(Span::styled(padded, theme::footer_muted()))
    }

    pub fn can_send_prompt(&self) -> bool {
        !self.transcript.is_live_activity()
            && !self.approval_open()
            && !self.composer.trim().is_empty()
            && !composer_is_slash_command(&self.composer)
    }

    pub fn sync_slash_palette(&mut self) {
        self.slash.sync(
            &self.composer,
            self.composer_focus && self.layout.focus == FocusRegion::Chat,
            &self.model_catalog,
        );
    }

    pub fn slash_palette_lines(
        &self,
        width: usize,
        max_rows: usize,
    ) -> Vec<ratatui::text::Line<'static>> {
        render_palette(
            &self.composer,
            self.slash.selected,
            width,
            max_rows,
            &self.model_catalog,
            &self.model_display,
            self.current_lht_composer_mode(),
        )
    }

    fn composer_editable(&self) -> bool {
        !self.approval_open()
            && !self.show_help
            && !self.transcript.is_live_activity()
            && self.layout.focus == FocusRegion::Chat
            && self.composer_focus
    }

    pub fn handle_composer_paste(&mut self, raw: &str) {
        if !self.composer_editable() {
            return;
        }
        let text = normalize_paste_text(raw);
        if text.is_empty() {
            return;
        }
        self.composer.push_str(&text);
        self.sync_slash_palette();
    }

    pub fn paste_from_clipboard(&mut self) -> bool {
        if !self.composer_editable() {
            return false;
        }
        let Some(raw) = read_clipboard_text() else {
            return false;
        };
        self.handle_composer_paste(&raw);
        true
    }

    pub fn handle_char(&mut self, ch: char) {
        if !self.composer_editable() {
            return;
        }
        if ch == '\n' || ch == '\r' {
            return;
        }
        self.composer.push(ch);
        self.sync_slash_palette();
    }

    pub fn handle_newline(&mut self) {
        if self.composer_editable() {
            self.composer.push('\n');
        }
    }

    pub fn handle_backspace(&mut self) {
        if self.layout.focus == FocusRegion::Chat && self.composer_focus && !self.approval_open() {
            self.composer.pop();
            self.sync_slash_palette();
        }
    }

    pub fn take_composer_prompt(&mut self) -> Option<String> {
        let text = self.composer.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.composer.clear();
        self.slash.close();
        Some(text)
    }

    pub fn push_system_line(&mut self, text: String) {
        self.transcript.items.push(TranscriptItem::System { text });
    }

    pub fn seed_resume_banner(&mut self) {
        if !self.transcript.items.is_empty() {
            self.transcript.items.insert(
                0,
                TranscriptItem::System {
                    text: format!("resumed thread {}", self.thread_id),
                },
            );
        }
    }

    pub async fn reload_after_thread_switch(&mut self, host: &TuiSessionHost) {
        self.thread_id = host.thread_id().to_string();
        self.workspace = host.thread.workspace.clone();
        self.layout.prefs.last_thread_id = Some(self.thread_id.clone());
        self.sync_thread_meta(host);
        self.transcript = TranscriptState::default();
        if let Ok(history) = host.load_history() {
            self.transcript.items = history;
        }
        self.agents.clear();
        self.pending_approval = None;
        self.blocked_line = None;
        self.task_graph = None;
        self.lht_pane_expanded = false;
        self.lht_auto_opened = false;
        self.lht_ui.reset();
        self.harness_line = title_bar_harness_line_from_graph(None);
        self.inspector
            .refresh_static(&host.thread.workspace, host.config());
        self.inspector_ui.reset_for_tab();
        self.refresh_panels(host).await;
        let threads = host.list_workspace_threads().await.unwrap_or_default();
        self.sessions = SessionList::from_threads(threads, host.thread_id());
    }

    pub fn poll_due(&self) -> bool {
        Instant::now() >= self.next_poll
    }

    pub fn schedule_next_poll(&mut self) {
        self.next_poll = Instant::now() + poll_interval(self.transcript.streaming);
    }

    pub async fn refresh_panels(&mut self, host: &TuiSessionHost) {
        if let Some(graph) = host.fetch_task_graph().await {
            self.apply_task_graph_snapshot(graph);
        } else if let Some(checklist) = host.fetch_checklist() {
            self.merge_checklist_snapshot(checklist);
        } else {
            self.task_graph = None;
            self.lht_pane_expanded = false;
            self.harness_line = title_bar_harness_line_from_graph(None);
        }
        self.context_pct = host.fetch_context_pct().await;
        self.inspector.agents = self.agents.clone();
        self.refresh_workspace_inspector(host);
        self.schedule_next_poll();
    }

    pub async fn refresh_sessions(&mut self, host: &TuiSessionHost) {
        if let Ok(threads) = host.list_workspace_threads().await {
            self.sessions = SessionList::from_threads(threads, host.thread_id());
        }
    }
}

fn format_run_mode_label(mode: &str, yolo: bool) -> String {
    if yolo || mode.eq_ignore_ascii_case("yolo") {
        "YOLO".to_string()
    } else if mode.eq_ignore_ascii_case("plan") {
        "Plan".to_string()
    } else {
        "Agent".to_string()
    }
}

fn format_task_type_label(task_type: &str) -> String {
    match task_type.to_ascii_lowercase().as_str() {
        "office" => "Office".to_string(),
        "code" => "Code".to_string(),
        "auto" => "Auto".to_string(),
        other if other.is_empty() => "Code".to_string(),
        other => other.to_string(),
    }
}

fn truncate_footer_workspace(text: &str, max_chars: usize) -> String {
    if max_chars < 4 || text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head = max_chars.saturating_sub(1);
    format!("{}…", text.chars().take(head).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_mode_labels_match_desktop() {
        assert_eq!(format_run_mode_label("agent", false), "Agent");
        assert_eq!(format_run_mode_label("yolo", false), "YOLO");
        assert_eq!(format_run_mode_label("plan", false), "Plan");
        assert_eq!(format_run_mode_label("agent", true), "YOLO");
    }

    #[test]
    fn task_type_labels_capitalize() {
        assert_eq!(format_task_type_label("code"), "Code");
        assert_eq!(format_task_type_label("office"), "Office");
    }

    #[test]
    fn composer_cjk_uses_single_span_without_width_overflow() {
        let app = AppState {
            layout: LayoutEngine::new(false, TuiLayoutPrefs::default()),
            transcript: TranscriptState::default(),
            composer: "你好世界".to_string(),
            composer_focus: true,
            thread_id: "t1".to_string(),
            workspace_display: String::new(),
            workspace: PathBuf::from("."),
            model_display: "m".to_string(),
            model_catalog: Vec::new(),
            run_mode_display: "Agent".to_string(),
            task_type_display: "Code".to_string(),
            lht_mode_display: "LHT Auto".to_string(),
            approval_display: "OnRequest".to_string(),
            approval_toggle_enabled: true,
            harness_line: String::new(),
            blocked_line: None,
            context_pct: None,
            sessions: SessionList::default(),
            inspector: InspectorCache::default(),
            inspector_ui: InspectorInteraction::default(),
            task_graph: None,
            lht_pane_expanded: false,
            lht_auto_opened: false,
            lht_ui: LhtPaneUi::default(),
            right_subfocus: RightSubfocus::Inspector,
            right_panel_height: 20,
            right_inspector_height: 20,
            right_lht_height: 0,
            agents: Vec::new(),
            pending_approval: None,
            show_help: false,
            terminal_resized: false,
            cursor_blink_since: Instant::now(),
            next_poll: Instant::now(),
            slash: SlashCommandState::default(),
        };
        let lines = app.composer_render(4, 30);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 2);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(display_width(&text) <= 30);
        assert!(text.contains('你'));
    }

    #[test]
    fn composer_empty_shows_blink_caret_line_when_focused() {
        let app = AppState {
            layout: LayoutEngine::new(false, TuiLayoutPrefs::default()),
            transcript: TranscriptState::default(),
            composer: String::new(),
            composer_focus: true,
            thread_id: "t1".to_string(),
            workspace_display: String::new(),
            workspace: PathBuf::from("."),
            model_display: "m".to_string(),
            model_catalog: Vec::new(),
            run_mode_display: "Agent".to_string(),
            task_type_display: "Code".to_string(),
            lht_mode_display: "LHT Auto".to_string(),
            approval_display: "OnRequest".to_string(),
            approval_toggle_enabled: true,
            harness_line: String::new(),
            blocked_line: None,
            context_pct: None,
            sessions: SessionList::default(),
            inspector: InspectorCache::default(),
            inspector_ui: InspectorInteraction::default(),
            task_graph: None,
            lht_pane_expanded: false,
            lht_auto_opened: false,
            lht_ui: LhtPaneUi::default(),
            right_subfocus: RightSubfocus::Inspector,
            right_panel_height: 20,
            right_inspector_height: 20,
            right_lht_height: 0,
            agents: Vec::new(),
            pending_approval: None,
            show_help: false,
            terminal_resized: false,
            cursor_blink_since: Instant::now(),
            next_poll: Instant::now(),
            slash: SlashCommandState::default(),
        };
        assert!(app.composer_shows_cursor());
        let lines = app.composer_render(4, 80);
        assert_eq!(lines.len(), 2);
        let caret = lines[1]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>();
        assert!(caret.starts_with("> -") || caret.starts_with(">  "));
    }
}
