//! Transcript state and engine event mapping.

use std::time::Instant;

use ratatui::text::Line;

use super::markdown_table::{AssistantBlock, split_assistant_blocks};
use super::theme::{self, AI_TAG, THINK_TAG, TOOL_TAG, USER_TAG};

use super::display_format::{
    pad_line_display_width, summarize_status_message, thinking_spinner_frame_at,
    thinking_status_line, tool_chain_status_line, wrap_display_line,
};
use super::transcript_filter::{
    format_tool_result_summary, format_tool_started_summary, sanitize_terminal_text,
};
use crate::core::events::{Event, TurnOutcomeStatus};

const TOOL_DETAIL_MAX: usize = 2048;
const BLOCK_GAP_LINES: usize = 4;
/// Extra blank rows between rendered transcript text lines (readability).
const LINE_GAP_ROWS: usize = 1;
const THINKING_PREVIEW_MAX: usize = 120;

/// Visual category for transcript coloring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranscriptLineKind {
    Spacer,
    User,
    Assistant,
    Thinking,
    ToolChain,
    System,
    Meta,
}

#[derive(Clone)]
struct LogicalLine {
    kind: TranscriptLineKind,
    text: String,
    table_rows: Option<Vec<Vec<String>>>,
    thinking_live: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptItem {
    User {
        text: String,
    },
    Assistant {
        text: String,
        streaming: bool,
    },
    Thinking {
        tail_preview: String,
        char_count: usize,
        streaming: bool,
    },
    Tool {
        id: String,
        name: String,
        summary: String,
        detail: String,
        expanded: bool,
        done: bool,
        success: Option<bool>,
    },
    System {
        text: String,
    },
    HarnessSystem {
        text: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct TranscriptState {
    pub items: Vec<TranscriptItem>,
    pub streaming: bool,
    pub end_reason: Option<String>,
    pub status_message: Option<String>,
    pub scroll_offset: usize,
    pub thinking_char_count: usize,
    pub thinking_anim_since: Option<Instant>,
    pub tool_chain_anim_since: Option<Instant>,
}

impl TranscriptState {
    pub fn push_user(&mut self, text: String) {
        self.items.push(TranscriptItem::User { text });
        self.scroll_offset = 0;
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    pub fn is_thinking(&self) -> bool {
        self.status_message
            .as_ref()
            .is_some_and(|m| m.starts_with("thinking"))
    }

    pub fn pending_tool_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item, TranscriptItem::Tool { done: false, .. }))
            .count()
    }

    pub fn is_tools_active(&self) -> bool {
        self.pending_tool_count() > 0
    }

    pub fn is_live_activity(&self) -> bool {
        self.streaming || self.is_thinking() || self.is_tools_active()
    }

    fn focus_pending_tool(&self) -> Option<(&str, &str)> {
        self.items.iter().rev().find_map(|item| match item {
            TranscriptItem::Tool {
                done: false,
                name,
                summary,
                ..
            } => Some((name.as_str(), summary.as_str())),
            _ => None,
        })
    }

    fn touch_tool_chain_anim(&mut self) {
        self.tool_chain_anim_since.get_or_insert_with(Instant::now);
        self.scroll_offset = 0;
    }

    fn maybe_clear_tool_chain_anim(&mut self) {
        if !self.is_tools_active() {
            self.tool_chain_anim_since = None;
        }
    }

    fn begin_thinking(&mut self) {
        if self.is_thinking() {
            return;
        }
        self.thinking_char_count = 0;
        self.thinking_anim_since = Some(Instant::now());
        self.status_message = Some("thinking".to_string());
        self.items.push(TranscriptItem::Thinking {
            tail_preview: String::new(),
            char_count: 0,
            streaming: true,
        });
        self.scroll_offset = 0;
    }

    fn finish_thinking(&mut self) {
        if !self.is_thinking() {
            return;
        }
        if let Some(TranscriptItem::Thinking {
            streaming,
            char_count,
            ..
        }) = self
            .items
            .iter_mut()
            .rev()
            .find(|i| matches!(i, TranscriptItem::Thinking { .. }))
        {
            *char_count = self.thinking_char_count.max(*char_count);
            *streaming = false;
        }
        self.status_message = None;
        self.thinking_anim_since = None;
        self.thinking_char_count = 0;
    }

    fn append_thinking_delta(&mut self, content: &str) {
        self.begin_thinking();
        self.thinking_char_count += content.chars().count();
        self.scroll_offset = 0;
        if let Some(TranscriptItem::Thinking {
            tail_preview,
            char_count,
            streaming,
        }) = self.items.last_mut()
        {
            if *streaming {
                *char_count = self.thinking_char_count;
                tail_preview.push_str(content);
                if tail_preview.chars().count() > THINKING_PREVIEW_MAX {
                    let skip = tail_preview.chars().count() - THINKING_PREVIEW_MAX;
                    *tail_preview = tail_preview.chars().skip(skip).collect();
                }
            }
        }
    }

    pub fn toggle_last_tool_expand(&mut self) {
        if let Some(TranscriptItem::Tool { expanded, .. }) = self
            .items
            .iter_mut()
            .rev()
            .find(|i| matches!(i, TranscriptItem::Tool { .. }))
        {
            *expanded = !*expanded;
        }
    }

    pub fn render_styled_lines(&self, max_lines: usize, max_cols: usize) -> Vec<Line<'static>> {
        let logical = dedupe_consecutive_tool_lines(self.flatten_logical_lines());
        let mut wrapped: Vec<LogicalLine> = Vec::new();
        for entry in logical {
            if entry.kind == TranscriptLineKind::Spacer {
                for _ in 0..BLOCK_GAP_LINES {
                    wrapped.push(LogicalLine {
                        kind: TranscriptLineKind::Spacer,
                        text: pad_line_display_width("", max_cols),
                        table_rows: None,
                        thinking_live: false,
                    });
                }
                continue;
            }
            let physical_lines = if let Some(rows) = &entry.table_rows {
                super::markdown_table::format_table(rows, max_cols)
            } else {
                wrap_display_line(&sanitize_terminal_text(&entry.text), max_cols)
            };
            for line in physical_lines {
                wrapped.push(LogicalLine {
                    kind: entry.kind,
                    text: pad_line_display_width(&line, max_cols),
                    table_rows: None,
                    thinking_live: entry.thinking_live,
                });
                if should_add_line_gap(entry.kind, &line) {
                    for _ in 0..LINE_GAP_ROWS {
                        wrapped.push(LogicalLine {
                            kind: TranscriptLineKind::Spacer,
                            text: pad_line_display_width("", max_cols),
                            table_rows: None,
                            thinking_live: false,
                        });
                    }
                }
            }
        }

        if wrapped.is_empty() {
            return vec![styled_line(
                TranscriptLineKind::Meta,
                "Transcript empty — type a prompt in Composer and press Enter.",
                false,
            )];
        }

        let max = max_lines.max(4);
        let window = if wrapped.len() <= max {
            wrapped
        } else {
            let end = wrapped.len().saturating_sub(self.scroll_offset);
            let start = end.saturating_sub(max);
            wrapped[start..end].to_vec()
        };

        window
            .into_iter()
            .map(|entry| styled_line(entry.kind, &entry.text, entry.thinking_live))
            .collect()
    }

    fn flatten_logical_lines(&self) -> Vec<LogicalLine> {
        let mut lines = Vec::new();
        for (idx, item) in self.items.iter().enumerate() {
            if idx > 0 {
                lines.push(LogicalLine::spacer());
            }
            let anim_since = match item {
                TranscriptItem::Thinking {
                    streaming: true, ..
                } => self.thinking_anim_since,
                TranscriptItem::Tool { done: false, .. } => self.tool_chain_anim_since,
                _ => None,
            };
            lines.extend(logical_lines_for_item(item, anim_since));
        }
        if self.is_tools_active() && !self.is_thinking() && self.status_message.is_none() {
            if !lines.is_empty() {
                lines.push(LogicalLine::spacer());
            }
            let pending = self.pending_tool_count();
            let focus_name = self
                .focus_pending_tool()
                .map(|(name, _)| name)
                .unwrap_or("tool");
            let display = tool_chain_status_line(pending, focus_name, self.tool_chain_anim_since);
            lines.push(LogicalLine::plain(
                TranscriptLineKind::ToolChain,
                format!("-- {display}"),
                true,
            ));
        }
        if let Some(msg) = &self.status_message {
            if !lines.is_empty() {
                lines.push(LogicalLine::spacer());
            }
            let thinking_live = msg.starts_with("thinking");
            let kind = if thinking_live {
                TranscriptLineKind::Thinking
            } else if msg.contains("approval") {
                TranscriptLineKind::ToolChain
            } else {
                TranscriptLineKind::Meta
            };
            let display = if thinking_live {
                thinking_status_line(self.thinking_char_count, self.thinking_anim_since)
            } else {
                msg.clone()
            };
            lines.push(LogicalLine::plain(
                kind,
                format!("-- {display}"),
                thinking_live,
            ));
        }
        lines
    }
}

impl LogicalLine {
    fn spacer() -> Self {
        Self {
            kind: TranscriptLineKind::Spacer,
            text: String::new(),
            table_rows: None,
            thinking_live: false,
        }
    }

    fn plain(kind: TranscriptLineKind, text: String, thinking_live: bool) -> Self {
        Self {
            kind,
            text,
            table_rows: None,
            thinking_live,
        }
    }

    fn table(kind: TranscriptLineKind, rows: Vec<Vec<String>>, thinking_live: bool) -> Self {
        Self {
            kind,
            text: String::new(),
            table_rows: Some(rows),
            thinking_live,
        }
    }
}

fn logical_lines_for_item(item: &TranscriptItem, anim_since: Option<Instant>) -> Vec<LogicalLine> {
    match item {
        TranscriptItem::User { text } => text
            .lines()
            .map(|line| {
                LogicalLine::plain(TranscriptLineKind::User, format!("{USER_TAG}{line}"), false)
            })
            .collect(),
        TranscriptItem::Assistant { text, streaming } => {
            let suffix = if *streaming { "_" } else { "" };
            if text.is_empty() && *streaming {
                return vec![LogicalLine::plain(
                    TranscriptLineKind::Assistant,
                    format!("{AI_TAG}...{suffix}"),
                    false,
                )];
            }
            let mut out = Vec::new();
            let blocks = split_assistant_blocks(text);
            let mut first_assistant_line = true;
            for (block_idx, block) in blocks.iter().enumerate() {
                if block_idx > 0 {
                    out.push(LogicalLine::spacer());
                }
                match block {
                    AssistantBlock::Table(rows) => {
                        out.push(LogicalLine::table(
                            TranscriptLineKind::Assistant,
                            rows.clone(),
                            false,
                        ));
                        first_assistant_line = false;
                    }
                    AssistantBlock::Prose(prose) => {
                        if prose.trim().is_empty() {
                            continue;
                        }
                        let prose_lines: Vec<&str> = prose.lines().collect();
                        let line_count = prose_lines.len();
                        for (i, line) in prose_lines.iter().enumerate() {
                            let prefix = if first_assistant_line && i == 0 {
                                AI_TAG
                            } else {
                                "    "
                            };
                            let tail = if *streaming
                                && block_idx + 1 == blocks.len()
                                && i + 1 == line_count
                            {
                                suffix
                            } else {
                                ""
                            };
                            out.push(LogicalLine::plain(
                                TranscriptLineKind::Assistant,
                                format!("{prefix}{line}{tail}"),
                                false,
                            ));
                        }
                        first_assistant_line = false;
                    }
                }
            }
            if text.is_empty() {
                out.push(LogicalLine::plain(
                    TranscriptLineKind::Assistant,
                    format!("{AI_TAG}{suffix}"),
                    false,
                ));
            }
            out
        }
        TranscriptItem::Thinking {
            tail_preview,
            char_count,
            streaming,
        } => {
            let mut out = vec![LogicalLine::plain(
                TranscriptLineKind::Thinking,
                if *streaming {
                    format!(
                        "{THINK_TAG}{}",
                        thinking_status_line(*char_count, anim_since)
                    )
                } else {
                    let count = super::transcript_filter::format_compact_count(*char_count);
                    format!("{THINK_TAG}reasoning done ({count})")
                },
                *streaming,
            )];
            let preview = tail_preview.trim();
            if *streaming && !preview.is_empty() {
                let collapsed: String = preview.split_whitespace().collect::<Vec<_>>().join(" ");
                out.push(LogicalLine::plain(
                    TranscriptLineKind::Thinking,
                    format!("     ..{collapsed}"),
                    true,
                ));
            }
            out
        }
        TranscriptItem::Tool {
            name,
            summary,
            detail,
            expanded,
            done,
            success,
            ..
        } => {
            let (mark, live) = if !*done {
                (
                    anim_since
                        .map(thinking_spinner_frame_at)
                        .unwrap_or("|")
                        .to_string(),
                    true,
                )
            } else {
                let mark = match success {
                    Some(true) => "+",
                    Some(false) => "x",
                    None => ".",
                };
                (mark.to_string(), false)
            };
            let mut out = vec![LogicalLine::plain(
                TranscriptLineKind::ToolChain,
                format!("{TOOL_TAG}{mark} {name}: {summary}"),
                live,
            )];
            if *expanded && !detail.is_empty() {
                for line in detail.lines().take(16) {
                    out.push(LogicalLine::plain(
                        TranscriptLineKind::ToolChain,
                        format!("    {line}"),
                        false,
                    ));
                }
            }
            out
        }
        TranscriptItem::System { text } => text
            .lines()
            .map(|line| LogicalLine::plain(TranscriptLineKind::System, format!("-- {line}"), false))
            .collect(),
        TranscriptItem::HarnessSystem { text } => text
            .lines()
            .map(|line| LogicalLine::plain(harness_line_kind(line), format!("-- {line}"), false))
            .collect(),
    }
}

fn should_add_line_gap(kind: TranscriptLineKind, line: &str) -> bool {
    if kind == TranscriptLineKind::Spacer {
        return false;
    }
    !super::markdown_table::is_table_render_line(line)
}

fn dedupe_consecutive_tool_lines(lines: Vec<LogicalLine>) -> Vec<LogicalLine> {
    let mut out: Vec<LogicalLine> = Vec::with_capacity(lines.len());
    for line in lines {
        let duplicate = line.kind == TranscriptLineKind::ToolChain
            && out.last().is_some_and(|last| {
                last.kind == TranscriptLineKind::ToolChain && last.text == line.text
            });
        if duplicate {
            continue;
        }
        out.push(line);
    }
    out
}

fn harness_line_kind(text: &str) -> TranscriptLineKind {
    if text.starts_with("reasoning complete") || text.starts_with("thinking") {
        TranscriptLineKind::Thinking
    } else if text.starts_with("harness:")
        || text.starts_with("status:")
        || text.starts_with("tool ")
    {
        TranscriptLineKind::ToolChain
    } else {
        TranscriptLineKind::Meta
    }
}

fn styled_line(kind: TranscriptLineKind, text: &str, thinking_live: bool) -> Line<'static> {
    theme::transcript_line(kind, text, thinking_live)
}

pub fn apply_event(state: &mut TranscriptState, event: Event) {
    match event {
        Event::MessageStarted { .. } => {
            state.finish_thinking();
            state.streaming = true;
            if !matches!(state.items.last(), Some(TranscriptItem::Assistant { .. })) {
                state.items.push(TranscriptItem::Assistant {
                    text: String::new(),
                    streaming: true,
                });
            }
        }
        Event::MessageDelta { content, .. } => {
            if probe_noise_line(&content) {
                return;
            }
            state.finish_thinking();
            state.streaming = true;
            state.scroll_offset = 0;
            match state.items.last_mut() {
                Some(TranscriptItem::Assistant { text, streaming }) => {
                    text.push_str(&content);
                    *streaming = true;
                }
                _ => {
                    state.items.push(TranscriptItem::Assistant {
                        text: content,
                        streaming: true,
                    });
                }
            }
        }
        Event::MessageComplete { .. } => {
            if let Some(TranscriptItem::Assistant { streaming, .. }) = state.items.last_mut() {
                *streaming = false;
            }
        }
        Event::ThinkingStarted { .. } => {
            state.begin_thinking();
        }
        Event::ThinkingDelta { content, .. } => {
            if probe_noise_line(&content) {
                return;
            }
            state.append_thinking_delta(&content);
        }
        Event::ThinkingComplete { .. } => {
            state.finish_thinking();
        }
        Event::ToolCallStarted { id, name, input } => {
            state.touch_tool_chain_anim();
            let detail = truncate_detail(&input.to_string());
            let summary = format_tool_started_summary(&name, &input);
            state.items.push(TranscriptItem::Tool {
                id,
                name,
                summary,
                detail,
                expanded: false,
                done: false,
                success: None,
            });
        }
        Event::ToolCallProgress { id, output } => {
            if output.trim().is_empty() {
                return;
            }
            state.touch_tool_chain_anim();
            let snippet = super::transcript_filter::truncate_plain(
                &sanitize_terminal_text(output.trim()),
                48,
            );
            let target = if id.is_empty() {
                state
                    .items
                    .iter_mut()
                    .rev()
                    .find(|item| matches!(item, TranscriptItem::Tool { done: false, .. }))
            } else {
                state.items.iter_mut().find(|item| {
                    matches!(item, TranscriptItem::Tool { id: tool_id, done: false, .. } if tool_id == &id)
                })
            };
            if let Some(TranscriptItem::Tool { summary, .. }) = target {
                let base = summary.split(" | ").next().unwrap_or(summary.as_str());
                *summary = format!("{base} | {snippet}");
            }
        }
        Event::ToolCallComplete {
            id, name, result, ..
        } => {
            update_tool_complete(state, &id, &name, result);
            state.maybe_clear_tool_chain_anim();
        }
        Event::TurnComplete {
            end_reason,
            status,
            error,
            ..
        } => {
            state.finish_thinking();
            state.tool_chain_anim_since = None;
            state.streaming = false;
            state.end_reason = end_reason.clone();
            if let Some(TranscriptItem::Assistant { streaming, .. }) = state.items.last_mut() {
                *streaming = false;
            }
            if let Some(reason) = end_reason.filter(|r| !r.trim().is_empty()) {
                state.items.push(TranscriptItem::HarnessSystem {
                    text: format!("turn end: {reason}"),
                });
            }
            if matches!(status, TurnOutcomeStatus::Failed) || error.is_some() {
                let msg = error.unwrap_or_else(|| format!("turn {status:?}"));
                state.items.push(TranscriptItem::System { text: msg });
            }
        }
        Event::Error { envelope, .. } => {
            state.streaming = false;
            state.items.push(TranscriptItem::System {
                text: envelope.message,
            });
        }
        Event::Status { message } => {
            if let Some(short) = summarize_status_message(&message) {
                state
                    .items
                    .push(TranscriptItem::HarnessSystem { text: short });
            }
        }
        Event::ApprovalRequired { tool_name, .. } => {
            state.status_message = Some(format!("approval required: {tool_name}"));
        }
        Event::CycleAdvanced { from, to, .. } => {
            state.items.push(TranscriptItem::HarnessSystem {
                text: format!("harness: cycle {from}→{to}"),
            });
        }
        Event::CraftVerdict { verdict, .. } => {
            state.items.push(TranscriptItem::HarnessSystem {
                text: format!("craft review: {verdict}"),
            });
        }
        Event::CraftBoardUpdated { .. } => {
            state.items.push(TranscriptItem::HarnessSystem {
                text: "blackboard findings updated".to_string(),
            });
        }
        Event::AgentSpawned { id, .. } => {
            state.items.push(TranscriptItem::HarnessSystem {
                text: format!("subagent spawned: {id}"),
            });
        }
        Event::AgentComplete { id, .. } => {
            state.items.push(TranscriptItem::HarnessSystem {
                text: format!("subagent done: {id}"),
            });
        }
        Event::AgentProgress { .. } => {}
        _ => {}
    }
}

fn update_tool_complete(
    state: &mut TranscriptState,
    id: &str,
    name: &str,
    result: Result<zagens_tools::ToolResult, zagens_tools::ToolError>,
) {
    if let Some(item) = state
        .items
        .iter_mut()
        .rev()
        .find(|i| matches!(i, TranscriptItem::Tool { id: tool_id, .. } if tool_id == id))
    {
        if let TranscriptItem::Tool {
            done,
            success,
            summary,
            detail,
            ..
        } = item
        {
            *done = true;
            match &result {
                Ok(output) => {
                    *success = Some(output.success);
                    if !output.content.is_empty() {
                        *summary =
                            format_tool_result_summary(name, &output.content, output.success);
                        detail.push_str("\n---\n");
                        detail.push_str(&truncate_detail(&output.content));
                    }
                }
                Err(err) => {
                    *success = Some(false);
                    *summary = err.to_string();
                }
            }
        }
    } else {
        let (done, success, summary, detail) = match result {
            Ok(output) => (
                true,
                Some(output.success),
                format_tool_result_summary(name, &output.content, output.success),
                truncate_detail(&output.content),
            ),
            Err(err) => (true, Some(false), err.to_string(), String::new()),
        };
        state.items.push(TranscriptItem::Tool {
            id: id.to_string(),
            name: name.to_string(),
            summary,
            detail,
            expanded: false,
            done,
            success,
        });
    }
}

pub fn seed_from_messages(
    messages: &[crate::models::Message],
    limit: usize,
) -> Vec<TranscriptItem> {
    let mut items = Vec::new();
    for message in messages
        .iter()
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let text = message
            .content
            .iter()
            .filter_map(|block| match block {
                crate::models::ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        if text.trim().is_empty() {
            continue;
        }
        match message.role.as_str() {
            "user" => items.push(TranscriptItem::User { text }),
            "assistant" => items.push(TranscriptItem::Assistant {
                text,
                streaming: false,
            }),
            _ => items.push(TranscriptItem::System { text }),
        }
    }
    items
}

fn probe_noise_line(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("[thinking-probe]")
        || t.starts_with("[lht-probe]")
        || t.starts_with("[stream-probe]")
        || t.contains("deltas=") && t.contains("flushes=") && t.contains("thread=thr_")
}

fn truncate_detail(text: &str) -> String {
    if text.chars().count() <= TOOL_DETAIL_MAX {
        text.to_string()
    } else {
        let cut: String = text.chars().take(TOOL_DETAIL_MAX).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::events::TurnOutcomeStatus;
    use zagens_core::models::Usage;

    #[test]
    fn thinking_delta_without_started_shows_thinking_block() {
        let mut state = TranscriptState::default();
        apply_event(
            &mut state,
            Event::ThinkingDelta {
                index: 0,
                content: "plan the answer".to_string(),
            },
        );
        assert!(state.is_thinking());
        assert!(state.items.iter().any(|i| matches!(
            i,
            TranscriptItem::Thinking {
                streaming: true,
                ..
            }
        )));
        apply_event(
            &mut state,
            Event::MessageDelta {
                index: 0,
                content: "hello".to_string(),
            },
        );
        assert!(!state.is_thinking());
        assert!(state.items.iter().any(|i| matches!(
            i,
            TranscriptItem::Thinking {
                streaming: false,
                ..
            }
        )));
        assert!(state.items.iter().any(|i| matches!(
            i,
            TranscriptItem::Assistant { text, .. } if text == "hello"
        )));
    }

    #[test]
    fn message_delta_appends_assistant_stream() {
        let mut state = TranscriptState::default();
        apply_event(
            &mut state,
            Event::MessageDelta {
                index: 0,
                content: "hello".to_string(),
            },
        );
        apply_event(
            &mut state,
            Event::MessageDelta {
                index: 0,
                content: " world".to_string(),
            },
        );
        assert_eq!(state.items.len(), 1);
        assert!(matches!(
            &state.items[0],
            TranscriptItem::Assistant { text, streaming } if text == "hello world" && *streaming
        ));
    }

    #[test]
    fn pending_tools_are_live_activity_with_spinner_line() {
        let mut state = TranscriptState::default();
        apply_event(
            &mut state,
            Event::ToolCallStarted {
                id: "t1".to_string(),
                name: "read_file".to_string(),
                input: serde_json::json!({"path": "a.rs"}),
            },
        );
        assert!(state.is_tools_active());
        assert!(state.is_live_activity());
        assert!(state.tool_chain_anim_since.is_some());
        let lines = state.render_styled_lines(20, 80);
        let joined: String = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("tool |") || joined.contains("tool /"));
        assert!(joined.contains("read_file"));
        assert!(joined.contains("tool running"));
    }

    #[test]
    fn tool_progress_updates_running_summary() {
        let mut state = TranscriptState::default();
        apply_event(
            &mut state,
            Event::ToolCallStarted {
                id: "t1".to_string(),
                name: "bash".to_string(),
                input: serde_json::json!({"command": "cargo test"}),
            },
        );
        apply_event(
            &mut state,
            Event::ToolCallProgress {
                id: "t1".to_string(),
                output: "running 12 tests".to_string(),
            },
        );
        assert!(matches!(
            &state.items[0],
            TranscriptItem::Tool { summary, done: false, .. }
            if summary.contains("running 12 tests")
        ));
    }

    #[test]
    fn tool_started_and_completed_update_block() {
        let mut state = TranscriptState::default();
        apply_event(
            &mut state,
            Event::ToolCallStarted {
                id: "t1".to_string(),
                name: "read_file".to_string(),
                input: serde_json::json!({"path": "a.rs"}),
            },
        );
        apply_event(
            &mut state,
            Event::ToolCallComplete {
                id: "t1".to_string(),
                name: "read_file".to_string(),
                result: Ok(zagens_tools::ToolResult::success("fn main() {}")),
            },
        );
        assert_eq!(state.items.len(), 1);
        assert!(matches!(
            &state.items[0],
            TranscriptItem::Tool {
                done: true,
                success: Some(true),
                summary,
                ..
            } if summary.contains("fn main")
        ));
    }

    #[test]
    fn tool_summary_uses_compact_format_not_raw_json() {
        let mut state = TranscriptState::default();
        apply_event(
            &mut state,
            Event::ToolCallStarted {
                id: "t1".to_string(),
                name: "web_search".to_string(),
                input: serde_json::json!({"query": "weather"}),
            },
        );
        assert!(matches!(
            &state.items[0],
            TranscriptItem::Tool { summary, .. } if summary.contains("weather") && !summary.contains('{')
        ));
    }

    #[test]
    fn turn_complete_clears_streaming_and_harness_line() {
        let mut state = TranscriptState::default();
        state.streaming = true;
        apply_event(
            &mut state,
            Event::TurnComplete {
                usage: Usage::default(),
                last_request_input_tokens: None,
                status: TurnOutcomeStatus::Completed,
                error: None,
                step_count: 1,
                tool_names: vec![],
                end_reason: Some("end_turn".to_string()),
            },
        );
        assert!(!state.streaming);
        assert!(state.items.iter().any(
            |i| matches!(i, TranscriptItem::HarnessSystem { text } if text.contains("turn end"))
        ));
    }

    #[test]
    fn assistant_markdown_table_renders_with_borders() {
        let mut state = TranscriptState::default();
        state.items.push(TranscriptItem::Assistant {
            text: "| 类别 | 模块 |\n|------|------|\n| 运行时核心 | runtime-server |".to_string(),
            streaming: false,
        });
        let lines = state.render_styled_lines(30, 100);
        let joined: String = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("+"),
            "expected table top border, got:\n{joined}"
        );
        assert!(joined.contains("类别"));
        assert!(joined.contains("runtime-server"));
    }

    #[test]
    fn render_applies_block_gaps_and_colors() {
        let mut state = TranscriptState::default();
        state.items.push(TranscriptItem::User {
            text: "hi".to_string(),
        });
        state.items.push(TranscriptItem::Assistant {
            text: "hello".to_string(),
            streaming: false,
        });
        let lines = state.render_styled_lines(40, 80);
        // user line + block gap + assistant line
        assert!(lines.len() >= 2 + BLOCK_GAP_LINES);
    }

    #[test]
    fn cycle_advanced_emits_harness_system_line() {
        let mut state = TranscriptState::default();
        apply_event(
            &mut state,
            Event::CycleAdvanced {
                from: 1,
                to: 2,
                briefing: zagens_core::cycle::CycleBriefing {
                    cycle: 1,
                    timestamp: chrono::Utc::now(),
                    briefing_text: String::new(),
                    token_estimate: 0,
                },
            },
        );
        assert!(matches!(
            &state.items[0],
            TranscriptItem::HarnessSystem { text } if text.contains("cycle 1→2")
        ));
    }
}
