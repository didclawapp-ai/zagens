//! Streaming response state and guardrails (P2 PR4 → `deepseek-core`).

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
