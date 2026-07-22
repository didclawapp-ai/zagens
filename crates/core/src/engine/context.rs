//! Context budgeting and prompt-shaping helpers for the engine (P2 PR4).

use zagens_tools::{EvidenceEnvelope, ToolResult};

use crate::chat::{
    DEFAULT_MAX_OUTPUT_TOKENS, Message, SystemPrompt, context_window_for_model,
    is_deepseek_v4_model, max_output_token_cap_for_model,
};
use crate::engine::token_estimate::TokenEstimator;

/// Max output tokens requested for normal agent turns.
pub const TURN_MAX_OUTPUT_TOKENS: u32 = 262_144;

const API_MAX_OUTPUT_TOKENS: u32 = DEFAULT_MAX_OUTPUT_TOKENS;

/// Compute the default `max_tokens` when the client did not supply an override.
#[must_use]
pub fn effective_max_output_tokens(model: &str) -> u32 {
    if is_deepseek_v4_model(model) {
        // Conservative default; desktop may override up to `DEEPSEEK_V4_MAX_OUTPUT_TOKENS`.
        API_MAX_OUTPUT_TOKENS
    } else {
        let window = context_window_for_model(model).unwrap_or(128_000);
        let capped = window / 2;
        capped.min(max_output_token_cap_for_model(model))
    }
}

/// Keep this many most recent messages when emergency trimming is required.
pub const MIN_RECENT_MESSAGES_TO_KEEP: usize = 4;
/// Allow a few emergency recovery attempts before failing the turn.
pub const MAX_CONTEXT_RECOVERY_ATTEMPTS: u8 = 2;
const CONTEXT_HEADROOM_TOKENS: usize = 1024;
const TOOL_RESULT_CONTEXT_HARD_LIMIT_CHARS: usize = 12_000;
const TOOL_RESULT_CONTEXT_SOFT_LIMIT_CHARS: usize = 2_000;
const TOOL_RESULT_CONTEXT_SNIPPET_CHARS: usize = 900;
const LARGE_CONTEXT_TOOL_RESULT_HARD_LIMIT_CHARS: usize = 180_000;
const LARGE_CONTEXT_TOOL_RESULT_SOFT_LIMIT_CHARS: usize = 60_000;
const LARGE_CONTEXT_TOOL_RESULT_SNIPPET_CHARS: usize = 40_000;
const LARGE_CONTEXT_WINDOW_TOKENS: u32 = 500_000;
const TOOL_RESULT_METADATA_SUMMARY_CHARS: usize = 320;

pub const COMPACTION_SUMMARY_MARKER: &str = "Conversation Summary (Auto-Generated)";

/// Messages-layer compaction block (P3). Distinct from seam `[ARCHIVED_CONTEXT]`.
pub const COMPACTED_HISTORY_MARKER: &str = "[COMPACTED_HISTORY]";

#[derive(Debug, Clone, Copy)]
struct ToolResultContextLimits {
    hard_limit_chars: usize,
    noisy_soft_limit_chars: usize,
    snippet_chars: usize,
}

#[must_use]
pub fn summarize_text(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let take = limit.saturating_sub(3);
    let mut out: String = text.chars().take(take).collect();
    out.push_str("...");
    out
}

fn summarize_text_head_tail(text: &str, limit: usize) -> String {
    let total = text.chars().count();
    if total <= limit {
        return text.to_string();
    }
    if limit <= 20 {
        return summarize_text(text, limit);
    }

    let marker = "\n\n[... output truncated for context ...]\n\n";
    let marker_len = marker.chars().count();
    if limit <= marker_len + 20 {
        return summarize_text(text, limit);
    }

    let remaining = limit - marker_len;
    let head_len = remaining.saturating_mul(2) / 3;
    let tail_len = remaining.saturating_sub(head_len);
    let head: String = text.chars().take(head_len).collect();
    let tail_vec: Vec<char> = text.chars().rev().take(tail_len).collect();
    let tail: String = tail_vec.into_iter().rev().collect();
    format!("{head}{marker}{tail}")
}

/// Whether a tool's results should compact earlier (graded noisy soft limit).
///
/// Built-in name list plus MCP (`mcp__` / `browser_*`) defaults. Tools may also
/// stamp `metadata.context_noisy=true` via [`ToolSpec::is_noisy`] in the registry.
#[must_use]
pub fn tool_result_is_noisy(tool_name: &str) -> bool {
    if tool_name.starts_with("mcp__")
        || tool_name.starts_with("browser_")
        || tool_name.starts_with("mcp_")
    {
        return true;
    }
    matches!(
        tool_name,
        "exec_shell"
            | "exec_shell_wait"
            | "exec_shell_interact"
            | "exec_wait"
            | "exec_interact"
            | "multi_tool_use.parallel"
            | "web_search"
            | "fetch_url"
            | "web.run"
            | "grep_files"
            | "glob_files"
            | "explore_codebase"
            | "investigate"
            | "answer_from_repo"
            | "change_and_verify"
            | "promote_to_context"
            | "run_tests"
            | "task_gate_run"
            | "browser_snapshot"
            | "browser_get_text"
            | "browser_console_tail"
    )
}

fn tool_result_marked_noisy(metadata: Option<&serde_json::Value>) -> bool {
    metadata
        .and_then(|m| m.get("context_noisy"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Soft limit for noisy tools is tighter than the hard limit (graded compact).
fn noisy_soft_limit_chars(limits: ToolResultContextLimits, tool_name: &str) -> usize {
    let base = limits.noisy_soft_limit_chars;
    // Extra-noisy shell / web dumps compact earlier than search.
    if matches!(
        tool_name,
        "exec_shell"
            | "exec_shell_wait"
            | "exec_wait"
            | "web_search"
            | "fetch_url"
            | "web.run"
            | "run_tests"
    ) {
        base.saturating_mul(3) / 4
    } else {
        base
    }
}

fn tool_result_metadata_summary(metadata: Option<&serde_json::Value>) -> Option<String> {
    let obj = metadata?.as_object()?;
    for key in ["summary", "stdout_summary", "stderr_summary", "message"] {
        if let Some(text) = obj.get(key).and_then(serde_json::Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(summarize_text(trimmed, TOOL_RESULT_METADATA_SUMMARY_CHARS));
            }
        }
    }
    None
}

fn summarize_subagent_status(status: &serde_json::Value) -> String {
    if let Some(raw) = status.as_str() {
        return raw.to_string();
    }
    if let Some(obj) = status.as_object()
        && let Some((kind, value)) = obj.iter().next()
    {
        if let Some(reason) = value.as_str().filter(|s| !s.trim().is_empty()) {
            return format!("{kind}({})", summarize_text(reason.trim(), 120));
        }
        return kind.to_string();
    }
    status.to_string()
}

fn summarize_subagent_snapshot(snapshot: &serde_json::Value, index: usize) -> String {
    let Some(obj) = snapshot.as_object() else {
        return format!(
            "- item {index}: {}",
            summarize_text(&snapshot.to_string(), 240)
        );
    };

    let agent_id = obj
        .get("agent_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let agent_type = obj
        .get("agent_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("agent");
    let status = obj
        .get("status")
        .map(summarize_subagent_status)
        .unwrap_or_else(|| "unknown".to_string());
    let objective = obj
        .get("assignment")
        .and_then(|assignment| assignment.get("objective"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| summarize_text(s, 220));
    let result = obj
        .get("result")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| summarize_text(s, 1_600));
    let steps = obj.get("steps_taken").and_then(serde_json::Value::as_u64);
    let duration_ms = obj.get("duration_ms").and_then(serde_json::Value::as_u64);

    let mut lines = vec![format!("- {agent_id} ({agent_type}) status={status}")];
    if let Some(objective) = objective {
        lines.push(format!("  objective: {objective}"));
    }
    match result {
        Some(result) => lines.push(format!("  result: {result}")),
        None => lines.push("  result: not available yet".to_string()),
    }
    if steps.is_some() || duration_ms.is_some() {
        let steps = steps
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".to_string());
        let duration_ms = duration_ms
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".to_string());
        lines.push(format!("  stats: steps={steps}, duration_ms={duration_ms}"));
    }
    lines.join("\n")
}

fn compact_subagent_tool_result_for_context(tool_name: &str, raw: &str) -> Option<String> {
    if !matches!(tool_name, "agent_result" | "agent_wait" | "wait") {
        return None;
    }

    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    let snapshots: Vec<&serde_json::Value> = match &parsed {
        serde_json::Value::Array(items) => items.iter().collect(),
        serde_json::Value::Object(_) => vec![&parsed],
        _ => return None,
    };

    let mut out = String::from("[sub-agent result summarized for parent context]\n");
    out.push_str("Use `agent_result` again only if you need the full raw payload.\n");
    for (idx, snapshot) in snapshots.iter().enumerate() {
        if idx >= 8 {
            out.push_str(&format!(
                "- ... {} more sub-agent result(s) omitted from context summary\n",
                snapshots.len().saturating_sub(idx)
            ));
            break;
        }
        out.push_str(&summarize_subagent_snapshot(snapshot, idx + 1));
        out.push('\n');
    }
    Some(out.trim_end().to_string())
}

fn tool_result_context_limits_for_model(model: &str) -> ToolResultContextLimits {
    let is_large_context =
        context_window_for_model(model).is_some_and(|window| window >= LARGE_CONTEXT_WINDOW_TOKENS);

    if is_large_context {
        ToolResultContextLimits {
            hard_limit_chars: LARGE_CONTEXT_TOOL_RESULT_HARD_LIMIT_CHARS,
            noisy_soft_limit_chars: LARGE_CONTEXT_TOOL_RESULT_SOFT_LIMIT_CHARS,
            snippet_chars: LARGE_CONTEXT_TOOL_RESULT_SNIPPET_CHARS,
        }
    } else {
        ToolResultContextLimits {
            hard_limit_chars: TOOL_RESULT_CONTEXT_HARD_LIMIT_CHARS,
            noisy_soft_limit_chars: TOOL_RESULT_CONTEXT_SOFT_LIMIT_CHARS,
            snippet_chars: TOOL_RESULT_CONTEXT_SNIPPET_CHARS,
        }
    }
}

#[must_use]
pub fn compact_tool_result_for_context(
    model: &str,
    tool_name: &str,
    output: &ToolResult,
) -> String {
    let raw = output.content.trim();
    if raw.is_empty() {
        return String::new();
    }

    if let Some(summary) = compact_subagent_tool_result_for_context(tool_name, raw) {
        return summary;
    }

    let limits = tool_result_context_limits_for_model(model);
    let raw_chars = raw.chars().count();
    let soft = noisy_soft_limit_chars(limits, tool_name);
    let noisy =
        tool_result_is_noisy(tool_name) || tool_result_marked_noisy(output.metadata.as_ref());
    let should_compact = raw_chars > limits.hard_limit_chars || (noisy && raw_chars > soft);
    let evidence = EvidenceEnvelope::from_metadata(output.metadata.as_ref());
    let evidence_ledger = evidence.as_ref().map(|env| env.format_ledger());

    // Always surface the evidence ledger so claims can be reconciled even when
    // the prose body is small enough to keep verbatim.
    if !should_compact {
        return match evidence_ledger {
            Some(ledger) => format!("{ledger}\n\n{raw}"),
            None => raw.to_string(),
        };
    }

    let snippet = summarize_text_head_tail(raw, limits.snippet_chars);
    let omitted = raw_chars.saturating_sub(snippet.chars().count());
    let summary = tool_result_metadata_summary(output.metadata.as_ref());

    let mut out = format!("[{tool_name} output compacted to protect context]\n");
    if let Some(ledger) = evidence_ledger {
        out.push_str(&ledger);
        out.push('\n');
        if matches!(
            evidence.map(|e| e.uncertainty).unwrap_or_default(),
            zagens_tools::UncertaintyKind::Truncated | zagens_tools::UncertaintyKind::Partial
        ) {
            out.push_str(
                "Do not invent unread ranges; re-call the tool with a narrower window or promote_to_context if a workshop-ref is present.\n",
            );
        }
    }
    if let Some(summary) = summary {
        out.push_str(&format!("Summary: {summary}\n"));
    }
    out.push_str(&format!(
        "Snippet: {snippet}\n(Original: {raw_chars} chars, omitted: {omitted} chars.)"
    ));
    out
}

#[must_use]
pub fn extract_compaction_summary_prompt(prompt: Option<SystemPrompt>) -> Option<SystemPrompt> {
    match prompt {
        Some(SystemPrompt::Blocks(blocks)) => {
            let summary_blocks: Vec<_> = blocks
                .into_iter()
                .filter(|block| block.text.contains(COMPACTION_SUMMARY_MARKER))
                .collect();
            if summary_blocks.is_empty() {
                None
            } else {
                Some(SystemPrompt::Blocks(summary_blocks))
            }
        }
        Some(SystemPrompt::Text(text)) => {
            if text.contains(COMPACTION_SUMMARY_MARKER) {
                Some(SystemPrompt::Text(text))
            } else {
                None
            }
        }
        None => None,
    }
}

/// Conservative full-request input token estimate.
///
/// Delegates to [`TokenEstimator`] (P2-B canonical path).  Formula:
/// `ceil(raw_message_tokens × 1.5) + system_tokens + framing`.
/// Thinking blocks are always counted (conservative capacity estimate).
#[must_use]
pub fn estimate_input_tokens_conservative(
    messages: &[Message],
    system: Option<&SystemPrompt>,
) -> usize {
    TokenEstimator.estimate_request_input(messages, system, true)
}

#[must_use]
pub fn context_input_budget(model: &str, requested_output_tokens: u32) -> Option<usize> {
    let window = usize::try_from(context_window_for_model(model)?).ok()?;
    let output = usize::try_from(requested_output_tokens).ok()?;
    window
        .checked_sub(output)
        .and_then(|v| v.checked_sub(CONTEXT_HEADROOM_TOKENS))
}

#[must_use]
pub fn turn_response_headroom_tokens() -> u64 {
    u64::from(TURN_MAX_OUTPUT_TOKENS).saturating_add(CONTEXT_HEADROOM_TOKENS as u64)
}

#[must_use]
pub fn is_context_length_error_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("maximum context length")
        || lower.contains("context length")
        || lower.contains("context_length")
        || lower.contains("prompt is too long")
        || (lower.contains("requested") && lower.contains("tokens") && lower.contains("maximum"))
        || lower.contains("context window")
}

/// Count how many oldest messages can be removed while keeping at least
/// [`MIN_RECENT_MESSAGES_TO_KEEP`] and staying within `target_input_budget`.
#[must_use]
pub fn count_oldest_messages_to_drain(
    messages: &[Message],
    system_prompt: Option<&SystemPrompt>,
    target_input_budget: usize,
) -> usize {
    let len = messages.len();
    if len <= MIN_RECENT_MESSAGES_TO_KEEP {
        return 0;
    }
    let max_drain = len - MIN_RECENT_MESSAGES_TO_KEEP;
    for drain in 1..=max_drain {
        if estimate_input_tokens_conservative(&messages[drain..], system_prompt)
            <= target_input_budget
        {
            return drain;
        }
    }
    max_drain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_budget_subtracts_output_and_headroom() {
        let budget =
            context_input_budget("deepseek-v4-pro", TURN_MAX_OUTPUT_TOKENS).expect("v4 window");
        let v4_window = context_window_for_model("deepseek-v4-pro").unwrap() as usize;
        let expected = v4_window
            .checked_sub(TURN_MAX_OUTPUT_TOKENS as usize)
            .and_then(|v| v.checked_sub(1_024))
            .unwrap();
        assert_eq!(budget, expected);
    }

    #[test]
    fn classifies_context_length_errors() {
        assert!(is_context_length_error_message(
            "maximum context length exceeded"
        ));
        assert!(!is_context_length_error_message("connection reset"));
    }

    #[test]
    fn count_oldest_messages_to_drain_returns_zero_at_min_keep() {
        use crate::chat::{ContentBlock, Message};

        let messages: Vec<Message> = (0..MIN_RECENT_MESSAGES_TO_KEEP)
            .map(|i| Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: format!("msg-{i}"),
                    cache_control: None,
                }],
            })
            .collect();
        assert_eq!(count_oldest_messages_to_drain(&messages, None, 1), 0);
    }

    #[test]
    fn count_oldest_messages_to_drain_batches_from_front() {
        use crate::chat::{ContentBlock, Message};

        let messages: Vec<Message> = (0..8)
            .map(|i| Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: "x".repeat(5000 + i),
                    cache_control: None,
                }],
            })
            .collect();
        let budget = estimate_input_tokens_conservative(&messages[4..], None) + 1;
        let drain = count_oldest_messages_to_drain(&messages, None, budget);
        assert!(drain >= 1);
        assert!(drain <= messages.len() - MIN_RECENT_MESSAGES_TO_KEEP);
        assert!(
            estimate_input_tokens_conservative(&messages[drain..], None) <= budget
                || drain == messages.len() - MIN_RECENT_MESSAGES_TO_KEEP
        );
    }

    #[test]
    fn compact_preserves_evidence_ledger() {
        use zagens_tools::{EvidenceCitation, EvidenceEnvelope, UncertaintyKind};

        let big = "y".repeat(20_000);
        let result = ToolResult::success(big).with_evidence(
            EvidenceEnvelope::new()
                .with_fact("total_matches", "7")
                .with_citation(EvidenceCitation::lines("src/lib.rs", 1, 3))
                .with_uncertainty(UncertaintyKind::Truncated),
        );
        let compacted = compact_tool_result_for_context("deepseek-chat", "grep_files", &result);
        assert!(compacted.contains("total_matches=7"));
        assert!(compacted.contains("src/lib.rs:1-3"));
        assert!(compacted.contains("Do not invent unread ranges"));
    }

    #[test]
    fn small_result_still_surfaces_evidence_ledger() {
        use zagens_tools::{EvidenceCitation, EvidenceEnvelope, UncertaintyKind};

        let result = ToolResult::success("hello").with_evidence(
            EvidenceEnvelope::new()
                .with_fact("path", "src/a.rs")
                .with_citation(EvidenceCitation::lines("src/a.rs", 1, 2))
                .with_uncertainty(UncertaintyKind::None),
        );
        let out = compact_tool_result_for_context("deepseek-chat", "read_file", &result);
        assert!(out.starts_with("[evidence uncertainty=none]"));
        assert!(out.contains("src/a.rs:1-2"));
        assert!(out.contains("hello"));
    }
}
