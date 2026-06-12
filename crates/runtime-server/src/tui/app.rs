//! TUI application state (Phase 2).

use std::time::Instant;

use super::display_format::{
    composer_cursor_blink_on, display_width, pad_line_display_width, truncate_display_width,
};
use super::theme::{self, COMPOSER_PROMPT};

use super::focus::FocusRegion;
use super::harness::{blocked_suffix, title_bar_harness_line};
use super::inspector::{AgentEntry, InspectorCache};
use super::layout::{InspectorTab, LayoutEngine, TuiLayoutPrefs};
use super::left_rail::SessionList;
use super::overlay::PendingApproval;
use super::poll::poll_interval;
use super::session_host::TuiSessionHost;
use super::transcript::{TranscriptItem, TranscriptState, apply_event};
use crate::core::events::Event;

pub struct AppState {
    pub layout: LayoutEngine,
    pub transcript: TranscriptState,
    pub composer: String,
    pub composer_focus: bool,
    pub thread_id: String,
    pub workspace_display: String,
    pub model_display: String,
    pub run_mode_display: String,
    pub task_type_display: String,
    pub approval_display: String,
    pub approval_toggle_enabled: bool,
    pub harness_line: String,
    pub blocked_line: Option<String>,
    pub context_pct: Option<u8>,
    pub sessions: SessionList,
    pub inspector: InspectorCache,
    pub agents: Vec<AgentEntry>,
    pub pending_approval: Option<PendingApproval>,
    pub show_help: bool,
    pub checklist_auto_opened: bool,
    pub cursor_blink_since: Instant,
    next_poll: Instant,
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
        let checklist = host.fetch_checklist();
        inspector.checklist = checklist.clone();

        let mut state = Self {
            layout: LayoutEngine::new(inline_mode, layout_prefs),
            transcript,
            composer: String::new(),
            composer_focus: true,
            thread_id: host.thread_id().to_string(),
            workspace_display: host.workspace_display(),
            model_display: String::new(),
            run_mode_display: String::new(),
            task_type_display: String::new(),
            approval_display: String::new(),
            approval_toggle_enabled: false,
            harness_line: title_bar_harness_line(checklist.as_ref()),
            blocked_line: None,
            context_pct: None,
            sessions,
            inspector,
            agents: Vec::new(),
            pending_approval: None,
            show_help: false,
            checklist_auto_opened: checklist.is_some(),
            cursor_blink_since: Instant::now(),
            next_poll: Instant::now(),
        };
        state.sync_thread_meta(host);
        state
    }

    pub fn sync_thread_meta(&mut self, host: &TuiSessionHost) {
        self.model_display = host.thread.model.clone();
        self.run_mode_display = format_run_mode_label(&host.thread.mode, host.yolo);
        self.task_type_display = format_task_type_label(&host.thread.task_type);
        self.workspace_display = host.workspace_display();
        let (approval, toggle) = host.approval_footer_meta();
        self.approval_display = approval;
        self.approval_toggle_enabled = toggle;
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
        self.transcript.push_user(text);
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

        let hint_style = theme::hint();
        let text_style = if self.composer_focus {
            theme::composer_input()
        } else {
            theme::composer_idle()
        };
        let show_cursor = self.composer_shows_cursor();
        let cursor_on = show_cursor && composer_cursor_blink_on(self.cursor_blink_since);

        if self.composer.is_empty() {
            let hint = if self.transcript.is_live_activity() {
                "> waiting for reply...  Ctrl+C interrupt  Esc scroll  Up/Down history"
            } else {
                "> type prompt...  Enter send  Shift+Enter newline  Esc scroll  Up/Down history"
            };
            let mut lines = vec![Line::from(Span::styled(
                pad_line_display_width(hint, max_cols),
                hint_style,
            ))];
            if show_cursor {
                let caret = if cursor_on {
                    format!("{COMPOSER_PROMPT}-")
                } else {
                    format!("{COMPOSER_PROMPT} ")
                };
                lines.push(Line::from(Span::styled(
                    pad_line_display_width(&caret, max_cols),
                    text_style,
                )));
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
                Line::from(Span::styled(padded, text_style))
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
        if super::display_format::display_width(&plain) <= max_cols {
            return line;
        }

        let compact = format!(
            "{} · {} · {} · approve:{}",
            self.model_display,
            self.run_mode_display,
            self.task_type_display,
            self.approval_display
        );
        let padded = super::display_format::pad_line_display_width(&compact, max_cols);
        Line::from(Span::styled(padded, theme::footer_muted()))
    }

    pub fn can_send_prompt(&self) -> bool {
        !self.transcript.is_live_activity()
            && !self.approval_open()
            && !self.composer.trim().is_empty()
    }

    pub fn handle_char(&mut self, ch: char) {
        if self.approval_open() || self.show_help || self.transcript.is_live_activity() {
            return;
        }
        if self.layout.focus != FocusRegion::Chat || !self.composer_focus {
            return;
        }
        if ch == '\n' || ch == '\r' {
            return;
        }
        self.composer.push(ch);
    }

    pub fn handle_newline(&mut self) {
        if self.approval_open() || self.show_help || self.transcript.is_live_activity() {
            return;
        }
        if self.layout.focus == FocusRegion::Chat && self.composer_focus {
            self.composer.push('\n');
        }
    }

    pub fn handle_backspace(&mut self) {
        if self.layout.focus == FocusRegion::Chat && self.composer_focus && !self.approval_open() {
            self.composer.pop();
        }
    }

    pub fn take_composer_prompt(&mut self) -> Option<String> {
        let text = self.composer.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.composer.clear();
        Some(text)
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
        self.sync_thread_meta(host);
        self.transcript = TranscriptState::default();
        if let Ok(history) = host.load_history() {
            self.transcript.items = history;
        }
        self.agents.clear();
        self.pending_approval = None;
        self.blocked_line = None;
        self.inspector
            .refresh_static(&host.thread.workspace, host.config());
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
        let checklist = host.fetch_checklist();
        if checklist.is_some() && !self.checklist_auto_opened {
            self.layout.prefs.set_inspector_tab(InspectorTab::Checklist);
            self.checklist_auto_opened = true;
        }
        self.inspector.checklist = checklist.clone();
        self.harness_line = title_bar_harness_line(checklist.as_ref());
        self.context_pct = host.fetch_context_pct().await;
        self.inspector.agents = self.agents.clone();
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
            model_display: "m".to_string(),
            run_mode_display: "Agent".to_string(),
            task_type_display: "Code".to_string(),
            approval_display: "Ask".to_string(),
            approval_toggle_enabled: true,
            harness_line: String::new(),
            blocked_line: None,
            context_pct: None,
            sessions: SessionList::default(),
            inspector: InspectorCache::default(),
            agents: Vec::new(),
            pending_approval: None,
            show_help: false,
            checklist_auto_opened: false,
            cursor_blink_since: Instant::now(),
            next_poll: Instant::now(),
        };
        let lines = app.composer_render(4, 30);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 1);
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
            model_display: "m".to_string(),
            run_mode_display: "Agent".to_string(),
            task_type_display: "Code".to_string(),
            approval_display: "Ask".to_string(),
            approval_toggle_enabled: true,
            harness_line: String::new(),
            blocked_line: None,
            context_pct: None,
            sessions: SessionList::default(),
            inspector: InspectorCache::default(),
            agents: Vec::new(),
            pending_approval: None,
            show_help: false,
            checklist_auto_opened: false,
            cursor_blink_since: Instant::now(),
            next_poll: Instant::now(),
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
