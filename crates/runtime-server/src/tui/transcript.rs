//! Transcript state and engine event mapping.

use std::time::Instant;

use ratatui::text::Line;

use super::theme::{self, AI_TAG, THINK_TAG, TOOL_TAG, USER_TAG};
use super::transcript_turn::{TranscriptTurn, TurnTool};

use super::display_format::{
    pad_line_display_width, summarize_status_message, thinking_spinner_frame_at,
    thinking_status_line, wrap_transcript_line,
};
use super::transcript_filter::{
    format_tool_result_summary, format_tool_started_summary, sanitize_terminal_text,
};
use crate::core::events::{Event, TurnOutcomeStatus};

const TOOL_DETAIL_MAX: usize = 2048;
/// Blank rows between conversation turns (user prompt → next user prompt).
const TURN_GAP_LINES: usize = 2;
/// Blank row between the user prompt and the agent's response (THK / tools / AI) in one turn.
/// Sections inside the agent block stay packed.
const USER_RESPONSE_GAP_LINES: usize = 1;
/// Blank row inserted after a collapsed/expanded thinking block, before the AI reply.
const THINKING_GAP_LINES: usize = 1;
/// How many lines of live thinking text to show while the model is still streaming its reasoning.
const THINKING_LIVE_LINES: usize = 3;
const THINKING_PREVIEW_MAX: usize = 120;
const THINKING_EXPAND_MAX: usize = 64;
const TOOL_DETAIL_EXPAND_LINES: usize = 16;

/// Visual category for transcript coloring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranscriptLineKind {
    Spacer,
    User,
    Assistant,
    Thinking,
    ToolChain,
    /// A finished tool call that failed (success == Some(false)) — warning tint.
    ToolError,
    /// Hard error / failed turn — red.
    System,
    /// Benign informational system line (model switch, new session, resume …) — muted.
    Notice,
    Meta,
}

/// Severity for `TranscriptItem::System` lines so info notices are not painted as errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemLevel {
    /// Informational (model/workspace switch, new session, resume banner, queued …).
    Info,
    /// Engine error or failed turn.
    Error,
}

#[derive(Clone)]
struct LogicalLine {
    kind: TranscriptLineKind,
    text: String,
    table_rows: Option<Vec<Vec<String>>>,
    thinking_live: bool,
    /// When `kind == Spacer`, how many blank rows to emit.
    gap_lines: usize,
    /// Language tag for code-fence lines (e.g. `"mermaid"`, `"rust"`, `""`).
    /// `None` means this is not a code-block line.
    code_lang: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptItem {
    Turn(TranscriptTurn),
    System { text: String, level: SystemLevel },
}

impl TranscriptItem {
    /// Informational system line (muted), e.g. model/workspace switch or resume banner.
    pub fn info(text: String) -> Self {
        TranscriptItem::System {
            text,
            level: SystemLevel::Info,
        }
    }

    /// Error system line (red), e.g. engine error or failed turn.
    pub fn error(text: String) -> Self {
        TranscriptItem::System {
            text,
            level: SystemLevel::Error,
        }
    }
}

#[derive(Clone, Default)]
pub struct TranscriptState {
    pub items: Vec<TranscriptItem>,
    pub streaming: bool,
    pub end_reason: Option<String>,
    pub status_message: Option<String>,
    pub scroll_offset: usize,
    pub thinking_char_count: usize,
    pub thinking_anim_since: Option<Instant>,
    pub tool_chain_anim_since: Option<Instant>,
    /// Anchor for marquee animation during any live turn phase (THK / tools / AI).
    live_activity_since: Option<Instant>,
    /// True while a user turn is open until `TurnComplete` or interrupt.
    pub open_turn: bool,
    render_epoch: u64,
    wrap_cache: Option<WrappedLayoutCache>,
    /// Source of the most recently seen mermaid code block (for browser-open).
    last_mermaid_src: Option<String>,
}

#[derive(Clone)]
struct WrappedLayoutCache {
    max_cols: usize,
    epoch: u64,
    lines: Vec<LogicalLine>,
}

impl TranscriptState {
    fn bump_render(&mut self) {
        self.render_epoch = self.render_epoch.wrapping_add(1);
        self.wrap_cache = None;
    }

    pub fn begin_turn(&mut self, user: String) {
        if self.open_turn {
            return;
        }
        self.bump_render();
        self.items
            .push(TranscriptItem::Turn(TranscriptTurn::new(user)));
        self.open_turn = true;
        self.live_activity_since = Some(Instant::now());
        self.scroll_offset = 0;
    }

    pub fn close_open_turn(&mut self) {
        if let Some(TranscriptItem::Turn(turn)) = self.items.last_mut() {
            turn.close();
        }
        self.open_turn = false;
        self.streaming = false;
        self.status_message = None;
        self.thinking_anim_since = None;
        self.tool_chain_anim_since = None;
        self.live_activity_since = None;
    }

    fn active_turn_mut(&mut self) -> Option<&mut TranscriptTurn> {
        match self.items.last_mut() {
            Some(TranscriptItem::Turn(turn)) if turn.open => Some(turn),
            _ => None,
        }
    }

    fn active_turn(&self) -> Option<&TranscriptTurn> {
        match self.items.last() {
            Some(TranscriptItem::Turn(turn)) if turn.open => Some(turn),
            _ => None,
        }
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Source code of the most recently seen mermaid code block, if any.
    pub fn last_mermaid_src(&self) -> Option<&str> {
        self.last_mermaid_src.as_deref()
    }

    pub fn is_thinking(&self) -> bool {
        self.status_message
            .as_ref()
            .is_some_and(|m| m.starts_with("thinking"))
    }

    pub fn pending_tool_count(&self) -> usize {
        self.active_turn()
            .map(|turn| turn.tools.iter().filter(|t| !t.done).count())
            .unwrap_or(0)
    }

    pub fn is_tools_active(&self) -> bool {
        self.pending_tool_count() > 0
    }

    pub fn is_live_activity(&self) -> bool {
        self.open_turn || self.streaming || self.is_thinking() || self.is_tools_active()
    }

    /// Short label for the activity marquee (Transcript ↔ Composer divider).
    pub fn activity_banner_label(&self) -> String {
        if self.is_thinking() {
            let spin = self
                .thinking_anim_since
                .map(thinking_spinner_frame_at)
                .unwrap_or("|");
            if self.thinking_char_count == 0 {
                return format!("{spin} 推理中 · THK");
            }
            return format!("{spin} 推理中 · THK · {} chars", self.thinking_char_count);
        }
        if let Some((name, _)) = self.focus_pending_tool() {
            let spin = self
                .tool_chain_anim_since
                .map(thinking_spinner_frame_at)
                .unwrap_or("|");
            return format!("{spin} 运行工具 · {name}");
        }
        if self.is_tools_active() {
            return "运行工具 · tools".to_string();
        }
        if self.streaming {
            return "生成回复 · AI".to_string();
        }
        "运行中 · …".to_string()
    }

    pub fn activity_anim_since(&self) -> Instant {
        self.thinking_anim_since
            .or(self.tool_chain_anim_since)
            .or(self.live_activity_since)
            .unwrap_or_else(Instant::now)
    }

    fn focus_pending_tool(&self) -> Option<(&str, &str)> {
        self.active_turn()?.tools.iter().rev().find_map(|tool| {
            if tool.done {
                None
            } else {
                Some((tool.name.as_str(), tool.summary.as_str()))
            }
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
        self.thinking_anim_since = Some(Instant::now());
        self.status_message = Some("thinking".to_string());
        if let Some(turn) = self.active_turn_mut() {
            turn.thinking.streaming = true;
        }
        self.scroll_offset = 0;
    }

    fn finish_thinking(&mut self) {
        if !self.is_thinking() {
            return;
        }
        let count = self.thinking_char_count;
        if let Some(turn) = self.active_turn_mut() {
            turn.thinking.char_count = count.max(turn.thinking.char_count);
            turn.thinking.streaming = false;
        }
        self.status_message = None;
        self.thinking_anim_since = None;
        self.thinking_char_count = 0;
    }

    fn append_thinking_delta(&mut self, content: &str) {
        self.begin_thinking();
        self.thinking_char_count += content.chars().count();
        self.scroll_offset = 0;
        let count = self.thinking_char_count;
        if let Some(turn) = self.active_turn_mut() {
            turn.thinking.streaming = true;
            turn.thinking.char_count = count;
            turn.thinking.text.push_str(content);
        }
    }

    pub fn toggle_last_turn_tools(&mut self) {
        self.bump_render();
        if let Some(TranscriptItem::Turn(turn)) = self.items.last_mut() {
            turn.tools_collapsed = !turn.tools_collapsed;
        }
    }

    pub fn toggle_last_turn_harness(&mut self) {
        self.bump_render();
        if let Some(TranscriptItem::Turn(turn)) = self.items.last_mut() {
            turn.harness_collapsed = !turn.harness_collapsed;
        }
    }

    pub fn last_turn_has_harness(&self) -> bool {
        self.items
            .last()
            .and_then(|item| match item {
                TranscriptItem::Turn(turn) => Some(!visible_harness_lines(turn).is_empty()),
                _ => None,
            })
            .unwrap_or(false)
    }

    /// All non-noise harness log lines across the transcript (Activity tab feed).
    pub fn harness_events(&self) -> Vec<String> {
        let mut out = Vec::new();
        for item in &self.items {
            let TranscriptItem::Turn(turn) = item else {
                continue;
            };
            for line in &turn.harness {
                if !is_transcript_noise_harness(line) {
                    out.push(line.clone());
                }
            }
        }
        out
    }

    pub fn harness_event_count(&self) -> usize {
        self.harness_events().len()
    }

    pub fn toggle_last_turn_detail(&mut self) {
        self.bump_render();
        let Some(TranscriptItem::Turn(turn)) = self.items.last_mut() else {
            return;
        };
        if !turn.tools.is_empty() && turn.tools_collapsed {
            turn.tools_collapsed = false;
            return;
        }
        if visible_harness_lines(turn).len() > 0 && turn.harness_collapsed {
            turn.harness_collapsed = false;
            return;
        }
        if !turn.thinking.streaming && !turn.thinking.text.trim().is_empty() {
            turn.thinking_expanded = !turn.thinking_expanded;
            return;
        }
        if let Some(tool) = turn.tools.last_mut() {
            tool.expanded = !tool.expanded;
        }
    }

    pub fn render_styled_lines(&mut self, max_lines: usize, max_cols: usize) -> Vec<Line<'static>> {
        let wrapped = self.wrapped_physical_lines(max_cols);

        if wrapped.is_empty() {
            return vec![styled_line(
                TranscriptLineKind::Meta,
                "Transcript empty - type a prompt in Composer and press Enter.",
                false,
                None,
            )];
        }

        let max = max_lines.max(4);
        let max_scroll = wrapped.len().saturating_sub(max);
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }
        let window = if wrapped.len() <= max {
            wrapped
        } else {
            let end = wrapped.len().saturating_sub(self.scroll_offset);
            let start = end.saturating_sub(max);
            wrapped[start..end].to_vec()
        };

        let spacer_needed = max.saturating_sub(window.len());
        window
            .into_iter()
            .map(|entry| {
                styled_line(
                    entry.kind,
                    &entry.text,
                    entry.thinking_live,
                    entry.code_lang.as_deref(),
                )
            })
            .chain(std::iter::repeat_n(
                styled_line(TranscriptLineKind::Spacer, "", false, None),
                spacer_needed,
            ))
            .collect()
    }

    fn wrapped_physical_lines(&mut self, max_cols: usize) -> Vec<LogicalLine> {
        if let Some(cache) = &self.wrap_cache {
            if cache.max_cols == max_cols && cache.epoch == self.render_epoch {
                return cache.lines.clone();
            }
        }
        let logical = dedupe_consecutive_tool_lines(self.flatten_logical_lines());
        let mut wrapped: Vec<LogicalLine> = Vec::new();
        for entry in logical {
            if entry.kind == TranscriptLineKind::Spacer {
                let gap = entry.gap_lines.max(1);
                for _ in 0..gap {
                    wrapped.push(LogicalLine {
                        kind: TranscriptLineKind::Spacer,
                        text: pad_line_display_width("", max_cols),
                        table_rows: None,
                        thinking_live: false,
                        gap_lines: 1,
                        code_lang: None,
                    });
                }
                continue;
            }
            // Code-block lines: truncate rather than word-wrap.
            if let Some(lang) = entry.code_lang.clone() {
                let sanitized = sanitize_terminal_text(&entry.text);
                wrapped.push(LogicalLine {
                    kind: entry.kind,
                    text: pad_line_display_width(&sanitized, max_cols),
                    table_rows: None,
                    thinking_live: false,
                    gap_lines: 0,
                    code_lang: Some(lang),
                });
                continue;
            }
            let physical_lines = if let Some(rows) = &entry.table_rows {
                super::markdown_table::format_table(rows, max_cols)
            } else {
                wrap_transcript_line(&sanitize_terminal_text(&entry.text), max_cols)
            };
            for line in physical_lines {
                wrapped.push(LogicalLine {
                    kind: entry.kind,
                    text: pad_line_display_width(&line, max_cols),
                    table_rows: None,
                    thinking_live: entry.thinking_live,
                    gap_lines: 0,
                    code_lang: None,
                });
            }
        }
        self.wrap_cache = Some(WrappedLayoutCache {
            max_cols,
            epoch: self.render_epoch,
            lines: wrapped.clone(),
        });
        wrapped
    }

    fn flatten_logical_lines(&self) -> Vec<LogicalLine> {
        let mut lines = Vec::new();
        for (idx, item) in self.items.iter().enumerate() {
            if idx > 0 && TURN_GAP_LINES > 0 {
                lines.push(LogicalLine::turn_spacer());
            }
            match item {
                TranscriptItem::Turn(turn) => append_turn_lines(&mut lines, turn, self),
                TranscriptItem::System { text, level } => {
                    lines.extend(logical_lines_for_system(text, *level));
                }
            }
        }
        lines
    }
}

fn append_turn_lines(lines: &mut Vec<LogicalLine>, turn: &TranscriptTurn, state: &TranscriptState) {
    lines.extend(logical_lines_for_user(&turn.user));

    let has_thinking = turn.thinking.streaming || !turn.thinking.text.trim().is_empty();
    let has_agent_section = has_thinking
        || !turn.tools.is_empty()
        || turn.content_streaming
        || !turn.content.trim().is_empty()
        || turn
            .harness
            .iter()
            .any(|line| !is_transcript_noise_harness(line));

    // One blank row separates the user prompt from the agent's work (THK / tools / AI).
    // Sections inside the agent block stay packed.
    if !turn.user.trim().is_empty() && has_agent_section {
        lines.push(LogicalLine::user_response_spacer());
    }

    if has_thinking {
        lines.extend(logical_lines_for_merged_thinking(
            &turn.thinking.text,
            turn.thinking.char_count,
            turn.thinking.streaming,
            turn.thinking_expanded,
            state.thinking_anim_since,
        ));
        // Add a visual gap after the thinking block so it doesn't run directly into
        // tool calls or the AI reply line.
        let has_content_after =
            !turn.tools.is_empty() || turn.content_streaming || !turn.content.trim().is_empty();
        if !turn.thinking.streaming && has_content_after {
            lines.push(LogicalLine::thinking_gap_spacer());
        }
    }

    if !turn.tools.is_empty() {
        if turn.tools_collapsed {
            lines.extend(logical_lines_for_tools_summary(
                &turn.tools,
                state.tool_chain_anim_since,
            ));
        } else {
            for tool in &turn.tools {
                let anim = if tool.done {
                    None
                } else {
                    state.tool_chain_anim_since
                };
                lines.extend(logical_lines_for_tool(tool, anim));
            }
        }
        // Gap after tools block so the AI reply doesn't run directly into the last tool line.
        let has_ai_content = turn.content_streaming || !turn.content.trim().is_empty();
        if has_ai_content {
            lines.push(LogicalLine::thinking_gap_spacer());
        }
    }

    if turn.content_streaming || !turn.content.trim().is_empty() {
        lines.extend(logical_lines_for_assistant(
            &turn.content,
            turn.content_streaming,
        ));
    }

    if !turn.harness.is_empty() {
        let visible = visible_harness_lines(turn);
        if turn.harness_collapsed {
            if !visible.is_empty() {
                lines.extend(logical_lines_for_harness_summary(&visible));
            }
        } else {
            for line in &visible {
                lines.extend(logical_lines_for_harness(line));
            }
        }
    }
}

fn visible_harness_lines(turn: &TranscriptTurn) -> Vec<String> {
    turn.harness
        .iter()
        .filter(|line| !is_transcript_noise_harness(line))
        .cloned()
        .collect()
}

fn logical_lines_for_harness_summary(lines: &[String]) -> Vec<LogicalLine> {
    let summary = summarize_harness_lines(lines);
    vec![LogicalLine::plain(
        TranscriptLineKind::Meta,
        format!("Harness · {summary} · Enter 展开"),
        false,
    )]
}

fn summarize_harness_lines(lines: &[String]) -> String {
    let n = lines.len();
    if n == 0 {
        return "0 events".to_string();
    }
    let mut parts = Vec::new();
    let sub = lines
        .iter()
        .filter(|l| l.to_ascii_lowercase().contains("subagent"))
        .count();
    if sub > 0 {
        parts.push(format!("{sub} subagent"));
    }
    let craft = lines
        .iter()
        .filter(|l| l.to_ascii_lowercase().contains("craft review"))
        .count();
    if craft > 0 {
        parts.push(format!("{craft} craft"));
    }
    let gate = lines
        .iter()
        .filter(|l| l.to_ascii_lowercase().contains("harness:"))
        .count();
    if gate > 0 {
        parts.push(format!("{gate} gate"));
    }
    if parts.is_empty() {
        format!("{n} events")
    } else {
        format!("{n} events ({})", parts.join(", "))
    }
}

fn logical_lines_for_merged_thinking(
    text: &str,
    char_count: usize,
    streaming: bool,
    expanded: bool,
    anim_since: Option<Instant>,
) -> Vec<LogicalLine> {
    let header = if streaming {
        format!(
            "{THINK_TAG}{}",
            thinking_status_line(char_count, anim_since)
        )
    } else if expanded {
        let count = super::transcript_filter::format_compact_count(char_count);
        format!("{THINK_TAG}推理 · {count} · 已展开，Enter 收起")
    } else {
        let count = super::transcript_filter::format_compact_count(char_count);
        format!("{THINK_TAG}推理 · {count} · 已收起，Enter 展开")
    };
    let mut out = vec![LogicalLine::plain(
        TranscriptLineKind::Thinking,
        header,
        streaming,
    )];
    if streaming {
        // While streaming: show the last THINKING_LIVE_LINES non-empty lines so the user
        // can read what the model is reasoning about in real-time.  Each ThinkingDelta
        // event bumps render_epoch → cache is cleared → this window slides forward.
        if !text.trim().is_empty() {
            let live: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
            let start = live.len().saturating_sub(THINKING_LIVE_LINES);
            for line in &live[start..] {
                let trimmed = line.trim();
                let preview =
                    super::transcript_filter::truncate_plain(trimmed, THINKING_PREVIEW_MAX);
                if !preview.is_empty() {
                    out.push(LogicalLine::plain(
                        TranscriptLineKind::Thinking,
                        format!("     {preview}"),
                        true, // thinking_live = true → bright color during streaming
                    ));
                }
            }
        }
        return out;
    }
    if text.trim().is_empty() {
        return out;
    }
    if expanded {
        let lines: Vec<&str> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        let total = lines.len();
        for line in lines.iter().take(THINKING_EXPAND_MAX) {
            out.push(LogicalLine::plain(
                TranscriptLineKind::Thinking,
                format!("     {line}"),
                false,
            ));
        }
        if total > THINKING_EXPAND_MAX {
            let remaining = total - THINKING_EXPAND_MAX;
            out.push(LogicalLine::plain(
                TranscriptLineKind::Thinking,
                format!("     … ({remaining} more lines)"),
                false,
            ));
        }
        return out;
    }
    let preview: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let preview = super::transcript_filter::truncate_plain(&preview, THINKING_PREVIEW_MAX);
    if !preview.is_empty() {
        out.push(LogicalLine::plain(
            TranscriptLineKind::Thinking,
            format!("     {preview}"),
            false,
        ));
    }
    out
}

fn logical_lines_for_tools_summary(
    tools: &[TurnTool],
    anim_since: Option<Instant>,
) -> Vec<LogicalLine> {
    let pending = tools.iter().any(|t| !t.done);
    let spin = if pending {
        anim_since
            .map(thinking_spinner_frame_at)
            .unwrap_or("|")
            .to_string()
    } else {
        String::new()
    };
    let summary = summarize_tool_chain(tools);
    let prefix = if pending {
        format!("{spin} ")
    } else {
        String::new()
    };
    vec![LogicalLine::plain(
        TranscriptLineKind::ToolChain,
        format!("{TOOL_TAG}{prefix}{summary} · Enter 展开"),
        pending,
    )]
}

fn summarize_tool_chain(tools: &[TurnTool]) -> String {
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    if names.is_empty() {
        return "0 tools".to_string();
    }
    if names.len() == 1 {
        return names[0].to_string();
    }
    let mut unique = Vec::new();
    for name in &names {
        if unique.last() != Some(name) {
            unique.push(*name);
        }
    }
    if unique.len() == 1 {
        return format!("{} ×{}", unique[0], names.len());
    }
    let head = unique
        .iter()
        .take(2)
        .copied()
        .collect::<Vec<_>>()
        .join(" · ");
    if unique.len() > 2 || names.len() > 2 {
        format!("{head} 等 {} 项", names.len())
    } else {
        head
    }
}

fn is_transcript_noise_harness(text: &str) -> bool {
    let t = text.trim();
    let lower = t.to_ascii_lowercase();
    lower.starts_with("status:")
        || lower.contains("auto-loaded deferred tool")
        || lower.contains("deferred tool")
        || lower == "running tools"
        || lower.starts_with("tool running")
}

impl LogicalLine {
    fn turn_spacer() -> Self {
        Self {
            kind: TranscriptLineKind::Spacer,
            text: String::new(),
            table_rows: None,
            thinking_live: false,
            gap_lines: TURN_GAP_LINES,
            code_lang: None,
        }
    }

    /// Single blank row between the user prompt and the agent's response block.
    fn user_response_spacer() -> Self {
        Self {
            kind: TranscriptLineKind::Spacer,
            text: String::new(),
            table_rows: None,
            thinking_live: false,
            gap_lines: USER_RESPONSE_GAP_LINES,
            code_lang: None,
        }
    }

    /// Single blank row between paragraphs (`\n\n`) inside one assistant block.
    fn prose_paragraph_spacer() -> Self {
        Self {
            kind: TranscriptLineKind::Spacer,
            text: String::new(),
            table_rows: None,
            thinking_live: false,
            gap_lines: 1,
            code_lang: None,
        }
    }

    /// Blank row after a thinking block, before tool calls or AI reply.
    fn thinking_gap_spacer() -> Self {
        Self {
            kind: TranscriptLineKind::Spacer,
            text: String::new(),
            table_rows: None,
            thinking_live: false,
            gap_lines: THINKING_GAP_LINES,
            code_lang: None,
        }
    }

    fn plain(kind: TranscriptLineKind, text: String, thinking_live: bool) -> Self {
        Self {
            kind,
            text,
            table_rows: None,
            thinking_live,
            gap_lines: 0,
            code_lang: None,
        }
    }

    fn table(kind: TranscriptLineKind, rows: Vec<Vec<String>>, thinking_live: bool) -> Self {
        Self {
            kind,
            text: String::new(),
            table_rows: Some(rows),
            thinking_live,
            gap_lines: 0,
            code_lang: None,
        }
    }

    fn code_line(kind: TranscriptLineKind, text: String, lang: String) -> Self {
        Self {
            kind,
            text,
            table_rows: None,
            thinking_live: false,
            gap_lines: 0,
            code_lang: Some(lang),
        }
    }
}

fn logical_lines_for_user(text: &str) -> Vec<LogicalLine> {
    text.lines()
        .map(|line| {
            LogicalLine::plain(TranscriptLineKind::User, format!("{USER_TAG}{line}"), false)
        })
        .collect()
}

fn logical_lines_for_assistant(text: &str, streaming: bool) -> Vec<LogicalLine> {
    let suffix = if streaming { "_" } else { "" };
    if text.is_empty() && streaming {
        return vec![LogicalLine::plain(
            TranscriptLineKind::Assistant,
            format!("{AI_TAG}...{suffix}"),
            false,
        )];
    }
    let mut out = Vec::new();
    let blocks = super::markdown_table::split_assistant_blocks(text);
    let mut first_assistant_line = true;
    for (block_idx, block) in blocks.iter().enumerate() {
        match block {
            super::markdown_table::AssistantBlock::Table(rows) => {
                out.push(LogicalLine::table(
                    TranscriptLineKind::Assistant,
                    rows.clone(),
                    false,
                ));
                first_assistant_line = false;
            }
            super::markdown_table::AssistantBlock::Code { lang, lines } => {
                // Header line: "    [lang]" or "    [code]" for unnamed fences.
                let label = if lang.is_empty() {
                    "code"
                } else {
                    lang.as_str()
                };
                let prefix = if first_assistant_line { AI_TAG } else { "    " };
                // Use the AI tag prefix for the very first block, but keep the header
                // dim — its kind is Assistant so the tag is rendered; the code_lang
                // field signals the renderer to use the fence-header style.
                out.push(LogicalLine::code_line(
                    TranscriptLineKind::Assistant,
                    format!("{prefix}[{label}]"),
                    format!("__header__{lang}"),
                ));
                first_assistant_line = false;
                for line in lines {
                    out.push(LogicalLine::code_line(
                        TranscriptLineKind::Assistant,
                        format!("    {line}"),
                        lang.clone(),
                    ));
                }
                // Trailing spacer so prose after the block gets a blank line.
                out.push(LogicalLine::prose_paragraph_spacer());
            }
            super::markdown_table::AssistantBlock::Prose(prose) => {
                if prose.trim().is_empty() {
                    continue;
                }
                let prose_lines: Vec<&str> = prose.lines().collect();
                let line_count = prose_lines.len();
                let mut pending_blank = false;
                for (i, line) in prose_lines.iter().enumerate() {
                    if line.trim().is_empty() {
                        // Collapse runs of blank source lines into one paragraph break;
                        // skip leading blanks (nothing emitted yet).
                        if !out.is_empty() {
                            pending_blank = true;
                        }
                        continue;
                    }
                    let prefix = if first_assistant_line { AI_TAG } else { "    " };
                    let tail = if streaming && block_idx + 1 == blocks.len() && i + 1 == line_count
                    {
                        suffix
                    } else {
                        ""
                    };
                    if pending_blank {
                        out.push(LogicalLine::prose_paragraph_spacer());
                        pending_blank = false;
                    }
                    out.push(LogicalLine::plain(
                        TranscriptLineKind::Assistant,
                        format!("{prefix}{line}{tail}"),
                        false,
                    ));
                    first_assistant_line = false;
                }
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

fn logical_lines_for_tool(tool: &TurnTool, anim_since: Option<Instant>) -> Vec<LogicalLine> {
    let (mark, live) = if !tool.done {
        (
            anim_since
                .map(thinking_spinner_frame_at)
                .unwrap_or("|")
                .to_string(),
            true,
        )
    } else {
        let mark = match tool.success {
            Some(true) => "+",
            Some(false) => "x",
            None => ".",
        };
        (mark.to_string(), false)
    };
    let header_kind = if tool.success == Some(false) {
        TranscriptLineKind::ToolError
    } else {
        TranscriptLineKind::ToolChain
    };
    let mut out = vec![LogicalLine::plain(
        header_kind,
        format!("{TOOL_TAG}{mark} {}: {}", tool.name, tool.summary),
        live,
    )];
    if tool.expanded && !tool.detail.is_empty() {
        let lines: Vec<&str> = tool.detail.lines().collect();
        let total = lines.len();
        for line in lines.iter().take(TOOL_DETAIL_EXPAND_LINES) {
            out.push(LogicalLine::plain(
                TranscriptLineKind::ToolChain,
                format!("    {line}"),
                false,
            ));
        }
        if total > TOOL_DETAIL_EXPAND_LINES {
            let remaining = total - TOOL_DETAIL_EXPAND_LINES;
            out.push(LogicalLine::plain(
                TranscriptLineKind::ToolChain,
                format!("    … ({remaining} more lines)"),
                false,
            ));
        }
    }
    out
}

fn logical_lines_for_system(text: &str, level: SystemLevel) -> Vec<LogicalLine> {
    let kind = match level {
        SystemLevel::Info => TranscriptLineKind::Notice,
        SystemLevel::Error => TranscriptLineKind::System,
    };
    text.lines()
        .map(|line| LogicalLine::plain(kind, format!("-- {line}"), false))
        .collect()
}

fn logical_lines_for_harness(text: &str) -> Vec<LogicalLine> {
    text.lines()
        .map(|line| LogicalLine::plain(harness_line_kind(line), format!("-- {line}"), false))
        .collect()
}

fn push_harness_line(state: &mut TranscriptState, text: String) {
    if is_transcript_noise_harness(&text) {
        return;
    }
    if let Some(turn) = state.active_turn_mut() {
        turn.harness.push(text);
    } else {
        state.items.push(TranscriptItem::info(text));
    }
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

fn styled_line(
    kind: TranscriptLineKind,
    text: &str,
    thinking_live: bool,
    code_lang: Option<&str>,
) -> Line<'static> {
    theme::transcript_line(kind, text, thinking_live, code_lang)
}

pub fn apply_event(state: &mut TranscriptState, event: Event) {
    state.bump_render();
    match event {
        Event::MessageStarted { .. } => {
            state.finish_thinking();
            state.streaming = true;
            if let Some(turn) = state.active_turn_mut() {
                if !turn.content.trim().is_empty() && !turn.content.ends_with("\n\n") {
                    if !turn.content.ends_with('\n') {
                        turn.content.push_str("\n\n");
                    } else {
                        turn.content.push('\n');
                    }
                }
                turn.content_streaming = true;
            }
        }
        Event::MessageDelta { content, .. } => {
            if probe_noise_line(&content) {
                return;
            }
            state.finish_thinking();
            state.streaming = true;
            state.scroll_offset = 0;
            if let Some(turn) = state.active_turn_mut() {
                turn.content.push_str(&content);
                turn.content_streaming = true;
            }
        }
        Event::MessageComplete { .. } => {
            if let Some(turn) = state.active_turn_mut() {
                turn.content_streaming = false;
                // Update last_mermaid_src from completed message content.
                state.last_mermaid_src = extract_last_mermaid(&turn.content.clone());
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
            if let Some(turn) = state.active_turn_mut() {
                turn.tools.push(TurnTool {
                    id,
                    name,
                    summary,
                    detail,
                    expanded: false,
                    done: false,
                    success: None,
                });
            }
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
            if let Some(turn) = state.active_turn_mut() {
                let target = if id.is_empty() {
                    turn.tools.iter_mut().rev().find(|t| !t.done)
                } else {
                    turn.tools.iter_mut().find(|t| t.id == id && !t.done)
                };
                if let Some(tool) = target {
                    let base = tool
                        .summary
                        .split(" | ")
                        .next()
                        .unwrap_or(tool.summary.as_str());
                    tool.summary = format!("{base} | {snippet}");
                }
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
            if let Some(turn) = state.active_turn_mut() {
                turn.content_streaming = false;
            }
            if let Some(reason) = end_reason.filter(|r| !r.trim().is_empty()) {
                push_harness_line(state, format!("turn end: {reason}"));
            }
            if matches!(status, TurnOutcomeStatus::Failed) || error.is_some() {
                let msg = error.unwrap_or_else(|| format!("turn {status:?}"));
                state.items.push(TranscriptItem::error(msg));
            }
            state.close_open_turn();
        }
        Event::Error { envelope, .. } => {
            state.streaming = false;
            state.items.push(TranscriptItem::error(envelope.message));
            state.close_open_turn();
        }
        Event::Status { message } => {
            if let Some(short) = summarize_status_message(&message) {
                push_harness_line(state, short);
            }
        }
        Event::ApprovalRequired { tool_name, .. } => {
            state.status_message = Some(format!("approval required: {tool_name}"));
        }
        Event::CycleAdvanced { from, to, .. } => {
            push_harness_line(state, format!("harness: cycle {from}->{to}"));
        }
        Event::CraftVerdict { verdict, .. } => {
            push_harness_line(state, format!("craft review: {verdict}"));
        }
        Event::CraftBoardUpdated { .. } => {
            push_harness_line(state, "blackboard findings updated".to_string());
        }
        Event::AgentSpawned { id, .. } => {
            push_harness_line(state, format!("subagent spawned: {id}"));
        }
        Event::AgentComplete { id, .. } => {
            push_harness_line(state, format!("subagent done: {id}"));
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
    let Some(turn) = state.active_turn_mut() else {
        return;
    };
    if let Some(tool) = turn.tools.iter_mut().find(|t| t.id == id) {
        tool.done = true;
        match &result {
            Ok(output) => {
                tool.success = Some(output.success);
                if !output.content.is_empty() {
                    tool.summary =
                        format_tool_result_summary(name, &output.content, output.success);
                    tool.detail.push_str("\n---\n");
                    tool.detail.push_str(&truncate_detail(&output.content));
                }
            }
            Err(err) => {
                tool.success = Some(false);
                tool.summary = err.to_string();
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
        turn.tools.push(TurnTool {
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
    let mut pending_user: Option<String> = None;

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
            "user" => {
                if let Some(user) = pending_user.take() {
                    let mut turn = TranscriptTurn::new(user);
                    turn.open = false;
                    items.push(TranscriptItem::Turn(turn));
                }
                pending_user = Some(text);
            }
            "assistant" => {
                if let Some(user) = pending_user.take() {
                    let mut turn = TranscriptTurn::new(user);
                    turn.content = text;
                    turn.open = false;
                    items.push(TranscriptItem::Turn(turn));
                } else {
                    let mut turn = TranscriptTurn::new(String::new());
                    turn.content = text;
                    turn.open = false;
                    items.push(TranscriptItem::Turn(turn));
                }
            }
            _ => items.push(TranscriptItem::info(text)),
        }
    }
    if let Some(user) = pending_user {
        let mut turn = TranscriptTurn::new(user);
        turn.open = false;
        items.push(TranscriptItem::Turn(turn));
    }
    items
}

pub(crate) fn probe_noise_line(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("[thinking-probe]")
        || t.starts_with("[lht-probe]")
        || t.starts_with("[stream-probe]")
        || t.contains("deltas=") && t.contains("flushes=") && t.contains("thread=thr_")
}

pub(crate) fn truncate_detail(text: &str) -> String {
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
    use crate::tui::transcript_turn::TurnThinking;
    use zagens_core::models::Usage;

    fn begin_test_turn(state: &mut TranscriptState, user: &str) {
        state.begin_turn(user.to_string());
    }

    fn active_turn<'a>(state: &'a TranscriptState) -> &'a TranscriptTurn {
        match state.items.last() {
            Some(TranscriptItem::Turn(turn)) => turn,
            _ => panic!("expected open turn"),
        }
    }

    fn render_joined(state: &mut TranscriptState, max_lines: usize, max_cols: usize) -> String {
        state
            .render_styled_lines(max_lines, max_cols)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn push_closed_turn(state: &mut TranscriptState, turn: TranscriptTurn) {
        state.items.push(TranscriptItem::Turn(turn));
    }

    #[test]
    fn thinking_delta_without_started_shows_thinking_block() {
        let mut state = TranscriptState::default();
        begin_test_turn(&mut state, "test");
        apply_event(
            &mut state,
            Event::ThinkingDelta {
                index: 0,
                content: "plan the answer".to_string(),
            },
        );
        assert!(state.is_thinking());
        assert!(active_turn(&state).thinking.streaming);
        apply_event(
            &mut state,
            Event::MessageDelta {
                index: 0,
                content: "hello".to_string(),
            },
        );
        assert!(!state.is_thinking());
        assert!(!active_turn(&state).thinking.streaming);
        assert_eq!(active_turn(&state).content, "hello");
    }

    #[test]
    fn message_delta_appends_assistant_stream() {
        let mut state = TranscriptState::default();
        begin_test_turn(&mut state, "test");
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
        let turn = active_turn(&state);
        assert_eq!(turn.content, "hello world");
        assert!(turn.content_streaming);
    }

    #[test]
    fn pending_tools_are_live_activity_with_spinner_line() {
        let mut state = TranscriptState::default();
        begin_test_turn(&mut state, "read");
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
        let joined = render_joined(&mut state, 20, 80);
        assert!(joined.contains("tool |") || joined.contains("tool /"));
        assert!(joined.contains("read_file"));
    }

    #[test]
    fn tool_progress_updates_running_summary() {
        let mut state = TranscriptState::default();
        begin_test_turn(&mut state, "test");
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
        assert!(
            active_turn(&state).tools[0]
                .summary
                .contains("running 12 tests")
        );
    }

    #[test]
    fn tool_started_and_completed_update_block() {
        let mut state = TranscriptState::default();
        begin_test_turn(&mut state, "test");
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
        let tool = &active_turn(&state).tools[0];
        assert!(tool.done);
        assert_eq!(tool.success, Some(true));
        assert!(tool.summary.contains("fn main"));
    }

    #[test]
    fn tool_summary_uses_compact_format_not_raw_json() {
        let mut state = TranscriptState::default();
        begin_test_turn(&mut state, "test");
        apply_event(
            &mut state,
            Event::ToolCallStarted {
                id: "t1".to_string(),
                name: "web_search".to_string(),
                input: serde_json::json!({"query": "weather"}),
            },
        );
        let summary = &active_turn(&state).tools[0].summary;
        assert!(summary.contains("weather") && !summary.contains('{'));
    }

    #[test]
    fn turn_complete_clears_streaming_and_harness_line() {
        let mut state = TranscriptState::default();
        begin_test_turn(&mut state, "test");
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
        assert!(!state.open_turn);
        let turn = match &state.items[0] {
            TranscriptItem::Turn(turn) => turn,
            _ => panic!("expected turn"),
        };
        assert!(turn.harness.iter().any(|t| t.contains("turn end")));
    }

    #[test]
    fn assistant_markdown_table_renders_with_borders() {
        let mut state = TranscriptState::default();
        let mut turn = TranscriptTurn::new("table".to_string());
        turn.content =
            "| 类别 | 模块 |\n|------|------|\n| 运行时核心 | runtime-server |".to_string();
        turn.open = false;
        push_closed_turn(&mut state, turn);
        let joined = render_joined(&mut state, 30, 100);
        assert!(
            joined.contains("┌"),
            "expected table top border, got:\n{joined}"
        );
        assert!(joined.contains("类别"));
        assert!(joined.contains("runtime-server"));
    }

    #[test]
    fn render_keeps_assistant_list_lines_compact() {
        let mut state = TranscriptState::default();
        let mut turn = TranscriptTurn::new("list".to_string());
        turn.content = "**代码工作**\n- item one\n- item two".to_string();
        turn.open = false;
        push_closed_turn(&mut state, turn);
        let joined = render_joined(&mut state, 40, 80);
        let lines: Vec<&str> = joined.lines().collect();
        let header_row = lines
            .iter()
            .position(|l| l.contains("代码工作"))
            .expect("header");
        let first_item = lines
            .iter()
            .position(|l| l.contains("item one"))
            .expect("item one");
        let second_item = lines
            .iter()
            .position(|l| l.contains("item two"))
            .expect("item two");
        assert_eq!(
            first_item,
            header_row + 1,
            "expected header immediately followed by first list item"
        );
        assert_eq!(
            second_item,
            first_item + 1,
            "expected list items on consecutive rows"
        );
    }

    #[test]
    fn render_keeps_single_blank_row_between_paragraphs() {
        let mut state = TranscriptState::default();
        let mut turn = TranscriptTurn::new("para".to_string());
        turn.content = "first paragraph\n\n\nsecond paragraph".to_string();
        turn.open = false;
        push_closed_turn(&mut state, turn);
        let joined = render_joined(&mut state, 40, 80);
        let lines: Vec<&str> = joined.lines().collect();
        let first = lines
            .iter()
            .position(|l| l.contains("first paragraph"))
            .expect("first");
        let second = lines
            .iter()
            .position(|l| l.contains("second paragraph"))
            .expect("second");
        assert_eq!(
            second,
            first + 2,
            "expected exactly one blank row separating paragraphs (multiple blanks collapse to one)"
        );
    }

    #[test]
    fn render_omits_section_gap_inside_turn() {
        let mut state = TranscriptState::default();
        let mut turn = TranscriptTurn::new("hi".to_string());
        turn.content = "hello".to_string();
        turn.open = false;
        push_closed_turn(&mut state, turn);
        let joined = render_joined(&mut state, 40, 80);
        let content_lines: Vec<&str> = joined
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        let user_idx = content_lines
            .iter()
            .position(|line| line.contains("you>"))
            .expect("user line");
        let ai_idx = content_lines
            .iter()
            .position(|line| line.contains("AI>"))
            .expect("assistant line");
        assert_eq!(
            ai_idx,
            user_idx + 1,
            "expected no blank spacer between user and assistant sections"
        );
    }

    #[test]
    fn turn_groups_desktop_order_user_thinking_tools_assistant() {
        let mut state = TranscriptState::default();
        let mut turn = TranscriptTurn::new("search news".to_string());
        turn.thinking = TurnThinking {
            text: "plan search".to_string(),
            char_count: 11,
            streaming: false,
        };
        turn.tools.push(TurnTool {
            id: "t1".to_string(),
            name: "web_search".to_string(),
            summary: "8 results".to_string(),
            detail: String::new(),
            expanded: false,
            done: true,
            success: Some(true),
        });
        turn.content = "Here is the news.".to_string();
        turn.harness
            .push("status: Auto-loaded deferred tool 'web_search'".to_string());
        turn.open = false;
        push_closed_turn(&mut state, turn);

        let joined = render_joined(&mut state, 40, 80);
        let user_pos = joined.find("you>").expect("user");
        let think_pos = joined.find("THK>").expect("thinking");
        let tool_pos = joined.find("tool ").expect("tool");
        let ai_pos = joined.find("AI>").expect("assistant");
        assert!(user_pos < think_pos);
        assert!(think_pos < tool_pos);
        assert!(tool_pos < ai_pos);
        assert!(!joined.contains("Auto-loaded deferred"));
        assert!(joined.contains("等") || joined.contains("web_search"));
    }

    #[test]
    fn interleaved_assistant_segments_merge_into_one_output_block() {
        let mut state = TranscriptState::default();
        begin_test_turn(&mut state, "go");
        apply_event(
            &mut state,
            Event::ThinkingDelta {
                index: 0,
                content: "phase 1".to_string(),
            },
        );
        apply_event(
            &mut state,
            Event::MessageDelta {
                index: 0,
                content: "step one".to_string(),
            },
        );
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
                result: Ok(zagens_tools::ToolResult::success("ok")),
            },
        );
        apply_event(
            &mut state,
            Event::ThinkingDelta {
                index: 0,
                content: "phase 2".to_string(),
            },
        );
        apply_event(
            &mut state,
            Event::MessageDelta {
                index: 0,
                content: "final answer".to_string(),
            },
        );

        let joined = render_joined(&mut state, 50, 100);
        let think_count = joined.matches("THK>").count();
        let ai_count = joined.matches("AI>").count();
        assert_eq!(
            think_count, 1,
            "expected one thinking header, got:\n{joined}"
        );
        assert_eq!(ai_count, 1, "expected one AI block, got:\n{joined}");
        assert!(joined.contains("step one"));
        assert!(joined.contains("final answer"));
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
            TranscriptItem::System { text, .. } if text.contains("cycle 1->2")
        ));
    }

    #[test]
    fn begin_turn_blocks_duplicate_user_until_complete() {
        let mut state = TranscriptState::default();
        state.begin_turn("first".to_string());
        state.begin_turn("second".to_string());
        assert_eq!(state.items.len(), 1);
        assert_eq!(active_turn(&state).user, "first");
    }
}

/// Extract the source of the last mermaid code block from an assistant message.
pub(crate) fn extract_last_mermaid(content: &str) -> Option<String> {
    let mut last: Option<String> = None;
    let blocks = super::markdown_table::split_assistant_blocks(content);
    for block in blocks {
        if let super::markdown_table::AssistantBlock::Code { lang, lines } = block {
            if lang.eq_ignore_ascii_case("mermaid") {
                last = Some(lines.join("\n"));
            }
        }
    }
    last
}
