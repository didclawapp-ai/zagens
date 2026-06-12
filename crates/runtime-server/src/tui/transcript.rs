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
/// Blank rows between sections inside one turn (user / THK / tools / AI).
const SECTION_GAP_LINES: usize = 0;
/// Blank rows between consecutive `\n`-separated lines in one assistant prose block (lists, headers).
const PROSE_LINE_GAP_LINES: usize = 0;
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
    /// When `kind == Spacer`, how many blank rows to emit.
    gap_lines: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptItem {
    Turn(TranscriptTurn),
    System { text: String },
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
    /// Anchor for marquee animation during any live turn phase (THK / tools / AI).
    live_activity_since: Option<Instant>,
    /// Collapsed tool-chain header (desktop MessageMetaBar default).
    pub tools_collapsed: bool,
    /// True while a user turn is open until `TurnComplete` or interrupt.
    pub open_turn: bool,
}

impl TranscriptState {
    pub fn begin_turn(&mut self, user: String) {
        if self.open_turn {
            return;
        }
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

    pub fn toggle_last_tool_expand(&mut self) {
        self.tools_collapsed = !self.tools_collapsed;
    }

    pub fn render_styled_lines(&self, max_lines: usize, max_cols: usize) -> Vec<Line<'static>> {
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
                    });
                }
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
                });
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
            .chain(std::iter::repeat(styled_line(TranscriptLineKind::Spacer, "", false)).take(max))
            .collect()
    }

    fn flatten_logical_lines(&self) -> Vec<LogicalLine> {
        let mut lines = Vec::new();
        for (idx, item) in self.items.iter().enumerate() {
            if idx > 0 && TURN_GAP_LINES > 0 {
                lines.push(LogicalLine::turn_spacer());
            }
            match item {
                TranscriptItem::Turn(turn) => append_turn_lines(&mut lines, turn, self),
                TranscriptItem::System { text } => {
                    lines.extend(logical_lines_for_system(text));
                }
            }
        }
        lines
    }
}

fn append_turn_lines(lines: &mut Vec<LogicalLine>, turn: &TranscriptTurn, state: &TranscriptState) {
    let mut section_idx = 0usize;
    let push_section_gap = |lines: &mut Vec<LogicalLine>, section_idx: &mut usize| {
        if *section_idx > 0 && SECTION_GAP_LINES > 0 {
            lines.push(LogicalLine::section_spacer());
        }
        *section_idx += 1;
    };

    push_section_gap(lines, &mut section_idx);
    lines.extend(logical_lines_for_user(&turn.user));

    let has_thinking = turn.thinking.streaming || !turn.thinking.text.trim().is_empty();
    if has_thinking {
        push_section_gap(lines, &mut section_idx);
        lines.extend(logical_lines_for_merged_thinking(
            &turn.thinking.text,
            turn.thinking.char_count,
            turn.thinking.streaming,
            state.thinking_anim_since,
        ));
    }

    if !turn.tools.is_empty() {
        push_section_gap(lines, &mut section_idx);
        if state.tools_collapsed {
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
    }

    if turn.content_streaming || !turn.content.trim().is_empty() {
        push_section_gap(lines, &mut section_idx);
        lines.extend(logical_lines_for_assistant(
            &turn.content,
            turn.content_streaming,
        ));
    }

    if !turn.harness.is_empty() {
        push_section_gap(lines, &mut section_idx);
        for line in &turn.harness {
            if is_transcript_noise_harness(line) {
                continue;
            }
            lines.extend(logical_lines_for_harness(line));
        }
    }
}

fn logical_lines_for_merged_thinking(
    text: &str,
    char_count: usize,
    streaming: bool,
    anim_since: Option<Instant>,
) -> Vec<LogicalLine> {
    let header = if streaming {
        format!(
            "{THINK_TAG}{}",
            thinking_status_line(char_count, anim_since)
        )
    } else {
        let count = super::transcript_filter::format_compact_count(char_count);
        format!("{THINK_TAG}推理 · {count} · 已收起，Enter 展开")
    };
    let mut out = vec![LogicalLine::plain(
        TranscriptLineKind::Thinking,
        header,
        streaming,
    )];
    if streaming || text.trim().is_empty() {
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
        }
    }

    fn section_spacer() -> Self {
        Self {
            kind: TranscriptLineKind::Spacer,
            text: String::new(),
            table_rows: None,
            thinking_live: false,
            gap_lines: SECTION_GAP_LINES,
        }
    }

    fn prose_line_spacer() -> Self {
        Self {
            kind: TranscriptLineKind::Spacer,
            text: String::new(),
            table_rows: None,
            thinking_live: false,
            gap_lines: PROSE_LINE_GAP_LINES,
        }
    }

    fn plain(kind: TranscriptLineKind, text: String, thinking_live: bool) -> Self {
        Self {
            kind,
            text,
            table_rows: None,
            thinking_live,
            gap_lines: 0,
        }
    }

    fn table(kind: TranscriptLineKind, rows: Vec<Vec<String>>, thinking_live: bool) -> Self {
        Self {
            kind,
            text: String::new(),
            table_rows: Some(rows),
            thinking_live,
            gap_lines: 0,
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
        if block_idx > 0 && SECTION_GAP_LINES > 0 {
            out.push(LogicalLine::section_spacer());
        }
        match block {
            super::markdown_table::AssistantBlock::Table(rows) => {
                out.push(LogicalLine::table(
                    TranscriptLineKind::Assistant,
                    rows.clone(),
                    false,
                ));
                first_assistant_line = false;
            }
            super::markdown_table::AssistantBlock::Prose(prose) => {
                if prose.trim().is_empty() {
                    continue;
                }
                let prose_lines: Vec<&str> = prose.lines().collect();
                let line_count = prose_lines.len();
                for (i, line) in prose_lines.iter().enumerate() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if i > 0 && PROSE_LINE_GAP_LINES > 0 {
                        out.push(LogicalLine::prose_line_spacer());
                    }
                    let prefix = if first_assistant_line && i == 0 {
                        AI_TAG
                    } else {
                        "    "
                    };
                    let tail = if streaming && block_idx + 1 == blocks.len() && i + 1 == line_count
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
    let mut out = vec![LogicalLine::plain(
        TranscriptLineKind::ToolChain,
        format!("{TOOL_TAG}{mark} {}: {}", tool.name, tool.summary),
        live,
    )];
    if tool.expanded && !tool.detail.is_empty() {
        for line in tool.detail.lines().take(16) {
            out.push(LogicalLine::plain(
                TranscriptLineKind::ToolChain,
                format!("    {line}"),
                false,
            ));
        }
    }
    out
}

fn logical_lines_for_system(text: &str) -> Vec<LogicalLine> {
    text.lines()
        .map(|line| LogicalLine::plain(TranscriptLineKind::System, format!("-- {line}"), false))
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
        state.items.push(TranscriptItem::System { text });
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

fn styled_line(kind: TranscriptLineKind, text: &str, thinking_live: bool) -> Line<'static> {
    theme::transcript_line(kind, text, thinking_live)
}

pub fn apply_event(state: &mut TranscriptState, event: Event) {
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
                state.items.push(TranscriptItem::System { text: msg });
            }
            state.close_open_turn();
        }
        Event::Error { envelope, .. } => {
            state.streaming = false;
            state.items.push(TranscriptItem::System {
                text: envelope.message,
            });
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
            push_harness_line(state, format!("harness: cycle {from}→{to}"));
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
            _ => items.push(TranscriptItem::System { text }),
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

    fn render_joined(state: &TranscriptState, max_lines: usize, max_cols: usize) -> String {
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
        let joined = render_joined(&state, 20, 80);
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
        let joined = render_joined(&state, 30, 100);
        assert!(
            joined.contains("+"),
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
        let joined = render_joined(&state, 40, 80);
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
    fn render_skips_markdown_blank_lines_in_prose() {
        let mut state = TranscriptState::default();
        let mut turn = TranscriptTurn::new("para".to_string());
        turn.content = "first paragraph\n\nsecond paragraph".to_string();
        turn.open = false;
        push_closed_turn(&mut state, turn);
        let joined = render_joined(&state, 40, 80);
        let lines: Vec<&str> = joined.lines().filter(|l| !l.trim().is_empty()).collect();
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
            first + 1,
            "expected no blank row between paragraphs"
        );
    }

    #[test]
    fn render_omits_section_gap_inside_turn() {
        let mut state = TranscriptState::default();
        let mut turn = TranscriptTurn::new("hi".to_string());
        turn.content = "hello".to_string();
        turn.open = false;
        push_closed_turn(&mut state, turn);
        let joined = render_joined(&state, 40, 80);
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

        let joined = render_joined(&state, 40, 80);
        let user_pos = joined.find("you>").expect("user");
        let think_pos = joined.find("THK>").expect("thinking");
        let tool_pos = joined.find("tool +").expect("tool");
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

        let joined = render_joined(&state, 50, 100);
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
            TranscriptItem::System { text } if text.contains("cycle 1→2")
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
