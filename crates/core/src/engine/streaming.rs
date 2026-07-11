//! Streaming response state and guardrails (P2 PR4 → `zagens-core`).

use crate::chat::ToolCaller;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentBlockKind {
    Text,
    Thinking,
    ToolUse,
}

#[derive(Debug, Clone)]
pub struct ToolUseState {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
    pub caller: Option<ToolCaller>,
    pub input_buffer: String,
}

pub const STREAM_CHUNK_TIMEOUT_SECS: u64 = 90;
pub const STREAM_MAX_CONTENT_BYTES: usize = 10 * 1024 * 1024;
pub const STREAM_MAX_DURATION_SECS: u64 = 1800;
pub const MAX_STREAM_ERRORS_BEFORE_FAIL: u32 = 5;
pub const MAX_TRANSPARENT_STREAM_RETRIES: u32 = 2;
/// Outer turn-step retries when a stream dies with no actionable content (#103).
pub const MAX_STREAM_RETRIES: u32 = 3;
/// Max consecutive auto-continuations after the model hits the output
/// `max_tokens` cap (`finish_reason=length`) with no tool call to carry the
/// turn. Bounds runaway cost / an infinite cut→continue loop while still
/// letting a genuinely huge answer (or reasoning) finish across several rounds.
/// Reset to 0 on any step that does not end in a length truncation.
pub const MAX_LENGTH_CONTINUATIONS: u32 = 8;
/// Max times a long-horizon turn that exhausts its `max_steps` budget may be
/// granted another full step window to keep pursuing an incomplete task graph,
/// instead of silently ending at the step cap (step-exhaustion early-stop).
/// Each grant extends the budget by the original `max_steps`; bounded so a
/// runaway task can't loop forever (e.g. 3 → up to 4× the base step budget).
pub const MAX_STEP_LIMIT_CONTINUATIONS: u32 = 3;
/// Max times a long-horizon turn whose [`LoopGuard`](crate::engine::loop_guard::LoopGuard)
/// halts (a tool failed `FAILURE_HALT_THRESHOLD` consecutive times) may be
/// granted a "change approach" continuation instead of silently ending the
/// turn as `Completed`. Kept small — a halt means the model is genuinely stuck,
/// so we reset the failure counters and nudge it to switch strategy at most
/// this many times before accepting the stop.
pub const MAX_LOOP_GUARD_CONTINUATIONS: u32 = 2;
/// Max times an in-flight turn whose context overflows the model budget (and
/// can't be brought back under it by emergency compaction within
/// [`MAX_CONTEXT_RECOVERY_ATTEMPTS`](crate::engine::context::MAX_CONTEXT_RECOVERY_ATTEMPTS))
/// may roll a long-horizon **cycle handoff** instead of hard-failing the turn.
/// A handoff swaps the bloated message buffer for a small `<carry_forward>`
/// briefing seed plus preserved structured state, so the next step starts with
/// room to spare. Kept tiny: if even a fresh briefing seed can't fit, the task
/// is genuinely too large and we fall back to the hard failure.
pub const MAX_CONTEXT_CYCLE_HANDOFFS: u32 = 2;
/// Max **clean** in-turn cycle advances at a per-step safe boundary. The cycle
/// threshold / long-horizon early-advance band is normally only evaluated
/// *between turns*; a long-horizon turn that loops many tool steps without
/// returning never reaches that boundary, so a turn crossing ~75% would only
/// get the hard-overflow emergency handoff ([`MAX_CONTEXT_CYCLE_HANDOFFS`]),
/// never a clean early refresh. Evaluating the gate after each completed tool
/// step closes that gap. Each clean advance resets context to a small briefing
/// seed (so the gate won't immediately re-fire); this bound is the safety net
/// against a pathological seed that itself stays over threshold. Generous —
/// a genuinely long turn may legitimately refresh several times.
pub const MAX_IN_TURN_CYCLE_ADVANCES: u32 = 8;

pub fn should_transparently_retry_stream(
    any_content_received: bool,
    transparent_attempts: u32,
    cancelled: bool,
) -> bool {
    !any_content_received && transparent_attempts < MAX_TRANSPARENT_STREAM_RETRIES && !cancelled
}

/// Whether the outer turn loop should re-issue the stream request after a
/// round that produced no sendable assistant content (no text, no tools).
///
/// Covers:
/// - stream errors with empty actionable body (including thinking-only)
/// - clean `upstream_eof` / `chunk_timeout` with zero content or thinking-only
///   (mid-reasoning idle-close → empty-body `Completed`)
///
/// Does **not** retry on user cancel, or when text/tools already landed.
pub fn should_outer_retry_empty_stream(
    stream_errors: u32,
    stream_end_reason: &str,
    has_tools: bool,
    has_text: bool,
    pending_message_complete: bool,
) -> bool {
    if has_tools || has_text || pending_message_complete {
        return false;
    }
    if stream_end_reason == "cancelled" {
        return false;
    }
    if stream_errors > 0 {
        return true;
    }
    matches!(stream_end_reason, "upstream_eof" | "chunk_timeout")
}

/// Model-facing length-truncation continuation copy, keyed by session locale.
///
/// Returns `(optional_assistant_placeholder, user_hint)`.
pub fn length_continuation_prompts(
    locale_tag: &str,
    had_sendable_assistant_content: bool,
) -> (Option<&'static str>, &'static str) {
    let zh = locale_tag
        .split(['-', '_'])
        .next()
        .unwrap_or(locale_tag)
        .eq_ignore_ascii_case("zh");
    if zh {
        let placeholder = if had_sendable_assistant_content {
            None
        } else {
            Some("(上一轮回复因达到输出长度上限被中断)")
        };
        let hint = if had_sendable_assistant_content {
            "[系统] 你上一条回复因达到输出长度上限被截断。请从中断处继续输出剩余内容，不要重复或重写已经输出的部分。"
        } else {
            "[系统] 你上一轮思考因达到输出长度上限被截断，尚未产出任何回复。请基于已有分析直接给出结论或下一步操作，并精简思考过程。"
        };
        (placeholder, hint)
    } else {
        let placeholder = if had_sendable_assistant_content {
            None
        } else {
            Some("(Previous reply was interrupted after hitting the output length limit)")
        };
        let hint = if had_sendable_assistant_content {
            "[system] Your previous reply was truncated by the output length limit. Continue from the interruption without repeating what was already written."
        } else {
            "[system] Your previous reasoning was truncated by the output length limit before any reply. Give the conclusion or next action based on what you already analyzed, and keep reasoning brief."
        };
        (placeholder, hint)
    }
}

/// Drop intermediate thinking deltas when the event channel is under backpressure
/// (previous send blocked ≥ this many ms). Content is still accumulated locally
/// and flushed on the next non-backpressured send / ThinkingComplete.
pub const THINKING_BACKPRESSURE_COALESCE_MS: u64 = 200;

pub const TOOL_CALL_START_MARKERS: [&str; 5] = [
    "[TOOL_CALL]",
    "<deepseek:tool_call",
    "<tool_call",
    "<invoke ",
    "<function_calls>",
];

pub const TOOL_CALL_END_MARKERS: [&str; 5] = [
    "[/TOOL_CALL]",
    "</deepseek:tool_call>",
    "</tool_call>",
    "</invoke>",
    "</function_calls>",
];

pub const FAKE_WRAPPER_NOTICE: &str =
    "Stripped non-API tool-call wrapper from model output (use the API tool channel)";

pub fn contains_fake_tool_wrapper(text: &str) -> bool {
    TOOL_CALL_START_MARKERS.iter().any(|m| text.contains(m))
}

fn find_first_marker(text: &str, markers: &[&str]) -> Option<(usize, usize)> {
    markers
        .iter()
        .filter_map(|marker| text.find(marker).map(|idx| (idx, marker.len())))
        .min_by_key(|(idx, _)| *idx)
}

pub fn filter_tool_call_delta(delta: &str, in_tool_call: &mut bool) -> String {
    if delta.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    let mut rest = delta;

    loop {
        if *in_tool_call {
            let Some((idx, len)) = find_first_marker(rest, &TOOL_CALL_END_MARKERS) else {
                break;
            };
            rest = &rest[idx + len..];
            *in_tool_call = false;
        } else {
            let Some((idx, len)) = find_first_marker(rest, &TOOL_CALL_START_MARKERS) else {
                output.push_str(rest);
                break;
            };
            output.push_str(&rest[..idx]);
            rest = &rest[idx + len..];
            *in_tool_call = true;
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outer_retry_on_stream_errors_with_empty_body() {
        assert!(should_outer_retry_empty_stream(
            1,
            "stream_event_break",
            false,
            false,
            false
        ));
        // Thinking-only + errors: still empty sendable body → retry.
        assert!(should_outer_retry_empty_stream(
            2,
            "stream_event_break",
            false,
            false,
            false
        ));
    }

    #[test]
    fn outer_retry_on_upstream_eof_zero_or_thinking_only() {
        assert!(should_outer_retry_empty_stream(
            0,
            "upstream_eof",
            false,
            false,
            false
        ));
        assert!(should_outer_retry_empty_stream(
            0,
            "chunk_timeout",
            false,
            false,
            false
        ));
    }

    #[test]
    fn outer_retry_skips_when_text_tools_or_cancel() {
        assert!(!should_outer_retry_empty_stream(
            0,
            "upstream_eof",
            true,
            false,
            false
        ));
        assert!(!should_outer_retry_empty_stream(
            0,
            "upstream_eof",
            false,
            true,
            false
        ));
        assert!(!should_outer_retry_empty_stream(
            1,
            "upstream_eof",
            false,
            false,
            true
        ));
        assert!(!should_outer_retry_empty_stream(
            0,
            "cancelled",
            false,
            false,
            false
        ));
        assert!(!should_outer_retry_empty_stream(
            0,
            "stream_event_break",
            false,
            false,
            false
        ));
    }

    #[test]
    fn length_continuation_prompts_follow_locale() {
        let (ph_zh, hint_zh) = length_continuation_prompts("zh-Hans", false);
        assert!(ph_zh.unwrap().contains("输出长度"));
        assert!(hint_zh.contains("[系统]"));

        let (ph_en, hint_en) = length_continuation_prompts("en", true);
        assert!(ph_en.is_none());
        assert!(hint_en.contains("[system]"));
        assert!(hint_en.contains("truncated"));
    }
}
