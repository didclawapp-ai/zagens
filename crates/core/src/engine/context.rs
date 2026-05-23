//! Context budgeting and prompt-shaping helpers for the engine (P2 PR4).

use deepseek_tools::ToolResult;

use crate::chat::{ContentBlock, Message, SystemPrompt, context_window_for_model};

/// Max output tokens requested for normal agent turns.
pub const TURN_MAX_OUTPUT_TOKENS: u32 = 262_144;

const API_MAX_OUTPUT_TOKENS: u32 = 65_536;

/// Compute the effective `max_tokens` to send in the API request for a given model.
#[must_use]
pub fn effective_max_output_tokens(model: &str) -> u32 {
    let window = context_window_for_model(model).unwrap_or(128_000);
    if window >= 500_000 {
        API_MAX_OUTPUT_TOKENS
    } else {
        let capped = window / 2;
        capped.min(API_MAX_OUTPUT_TOKENS)
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

fn tool_result_is_noisy(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "exec_shell"
            | "exec_shell_wait"
            | "exec_shell_interact"
            | "multi_tool_use.parallel"
            | "web_search"
    )
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
pub fn compact_tool_result_for_context(model: &str, tool_name: &str, output: &ToolResult) -> String {
    let raw = output.content.trim();
    if raw.is_empty() {
        return String::new();
    }

    if let Some(summary) = compact_subagent_tool_result_for_context(tool_name, raw) {
        return summary;
    }

    let limits = tool_result_context_limits_for_model(model);
    let raw_chars = raw.chars().count();
    let should_compact = raw_chars > limits.hard_limit_chars
        || (tool_result_is_noisy(tool_name) && raw_chars > limits.noisy_soft_limit_chars);
    if !should_compact {
        return raw.to_string();
    }

    let snippet = summarize_text_head_tail(raw, limits.snippet_chars);
    let omitted = raw_chars.saturating_sub(snippet.chars().count());
    let summary = tool_result_metadata_summary(output.metadata.as_ref());

    if let Some(summary) = summary {
        format!(
            "[{tool_name} output compacted to protect context]\nSummary: {summary}\nSnippet: {snippet}\n(Original: {raw_chars} chars, omitted: {omitted} chars.)"
        )
    } else {
        format!(
            "[{tool_name} output compacted to protect context]\nSnippet: {snippet}\n(Original: {raw_chars} chars, omitted: {omitted} chars.)"
        )
    }
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

fn estimate_text_tokens_conservative(text: &str) -> usize {
    text.chars().count().div_ceil(3)
}

fn estimate_block_tokens_conservative(block: &ContentBlock) -> usize {
    match block {
        ContentBlock::Text { text, .. } => estimate_text_tokens_conservative(text),
        ContentBlock::Thinking { thinking } => estimate_text_tokens_conservative(thinking),
        ContentBlock::ToolUse { input, .. } => {
            estimate_text_tokens_conservative(&input.to_string())
        }
        ContentBlock::ToolResult { content, .. } => estimate_text_tokens_conservative(content),
        ContentBlock::ServerToolUse { input, .. } => {
            estimate_text_tokens_conservative(&input.to_string())
        }
        ContentBlock::ToolSearchToolResult { content, .. } => {
            estimate_text_tokens_conservative(&content.to_string())
        }
        ContentBlock::CodeExecutionToolResult { content, .. } => {
            estimate_text_tokens_conservative(&content.to_string())
        }
    }
}

fn estimate_messages_tokens_conservative(messages: &[Message]) -> usize {
    messages
        .iter()
        .flat_map(|message| message.content.iter())
        .map(estimate_block_tokens_conservative)
        .sum()
}

fn estimate_system_tokens_conservative(system: Option<&SystemPrompt>) -> usize {
    match system {
        Some(SystemPrompt::Text(text)) => estimate_text_tokens_conservative(text),
        Some(SystemPrompt::Blocks(blocks)) => blocks
            .iter()
            .map(|block| estimate_text_tokens_conservative(&block.text))
            .sum(),
        None => 0,
    }
}

#[must_use]
pub fn estimate_input_tokens_conservative(
    messages: &[Message],
    system: Option<&SystemPrompt>,
) -> usize {
    let message_tokens = estimate_messages_tokens_conservative(messages)
        .saturating_mul(3)
        .div_ceil(2);
    let system_tokens = estimate_system_tokens_conservative(system);
    let framing_overhead = messages.len().saturating_mul(12).saturating_add(48);
    message_tokens
        .saturating_add(system_tokens)
        .saturating_add(framing_overhead)
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
    use deepseek_tools::ToolResult;

    #[test]
    fn context_budget_subtracts_output_and_headroom() {
        let budget = context_input_budget("deepseek-v4-pro", TURN_MAX_OUTPUT_TOKENS)
            .expect("v4 window");
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
}
