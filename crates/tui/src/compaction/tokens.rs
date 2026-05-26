use std::path::Path;

use crate::models::{ContentBlock, Message, SystemPrompt};
use deepseek_core::compaction::CompactionConfig;

use super::plan::plan_compaction;
use super::{KEEP_RECENT_MESSAGES, MIN_SUMMARIZE_MESSAGES};

pub(crate) fn estimate_tokens_for_message(message: &Message, include_thinking: bool) -> usize {
    message
        .content
        .iter()
        .map(|c| match c {
            ContentBlock::Text { text, .. } => estimate_text_tokens_deepseek(text),
            // Historical reasoning blocks are UI/session metadata for DeepSeek.
            // Only current-turn tool-call reasoning is sent back to the API.
            ContentBlock::Thinking { thinking } if include_thinking => {
                estimate_text_tokens_deepseek(thinking)
            }
            ContentBlock::Thinking { .. } => 0,
            ContentBlock::ToolUse { input, .. } => serde_json::to_string(input)
                .map(|s| estimate_text_tokens_deepseek(&s))
                .unwrap_or(100),
            ContentBlock::ToolResult { content, .. } => estimate_text_tokens_deepseek(content),
            ContentBlock::ServerToolUse { .. }
            | ContentBlock::ToolSearchToolResult { .. }
            | ContentBlock::CodeExecutionToolResult { .. } => 0,
        })
        .sum::<usize>()
}

/// DeepSeek API doc heuristic: ~0.3 token/ASCII char, ~0.6 token/CJK char.
/// <https://api-docs.deepseek.com/zh-cn/quick_start/token_usage>
#[must_use]
pub fn estimate_text_tokens_deepseek(text: &str) -> usize {
    let (cjk, other) = count_cjk_and_other_chars(text);
    other
        .saturating_mul(3)
        .div_ceil(10)
        .saturating_add(cjk.saturating_mul(6).div_ceil(10))
}

pub(crate) fn count_cjk_and_other_chars(text: &str) -> (usize, usize) {
    let mut cjk = 0usize;
    let mut other = 0usize;
    for ch in text.chars() {
        if is_cjk_char(ch) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    (cjk, other)
}

pub(crate) fn is_cjk_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{4e00}'..='\u{9fff}'
            | '\u{3400}'..='\u{4dbf}'
            | '\u{3000}'..='\u{303f}'
            | '\u{ff00}'..='\u{ffef}'
            | '\u{2e80}'..='\u{2fdf}'
    )
}

pub fn estimate_tokens(messages: &[Message]) -> usize {
    // Rough estimate: ~4 chars per token. DeepSeek thinking-mode rule: any
    // assistant message with tool_calls keeps its reasoning_content forever
    // (replayed in all subsequent requests). Final text-only answers drop it.
    messages
        .iter()
        .map(|message| estimate_tokens_for_message(message, message_has_tool_use(message)))
        .sum()
}

pub(crate) fn message_has_tool_use(message: &Message) -> bool {
    message
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolUse { .. }))
}

pub(crate) fn estimate_text_tokens_conservative(text: &str) -> usize {
    // Align with DeepSeek doc ratios, then round up one char-worth of budget.
    estimate_text_tokens_deepseek(text).saturating_add(1)
}

pub(crate) fn estimate_system_tokens_conservative(system: Option<&SystemPrompt>) -> usize {
    match system {
        Some(SystemPrompt::Text(text)) => estimate_text_tokens_conservative(text),
        Some(SystemPrompt::Blocks(blocks)) => blocks
            .iter()
            .map(|block| estimate_text_tokens_conservative(&block.text))
            .sum(),
        None => 0,
    }
}

/// Conservative estimate for full request input tokens (messages + system + framing).
#[must_use]
pub fn estimate_input_tokens_conservative(
    messages: &[Message],
    system: Option<&SystemPrompt>,
) -> usize {
    let message_tokens = estimate_tokens(messages).saturating_mul(3).div_ceil(2);
    let system_tokens = estimate_system_tokens_conservative(system);
    let framing_overhead = messages.len().saturating_mul(12).saturating_add(48);
    message_tokens
        .saturating_add(system_tokens)
        .saturating_add(framing_overhead)
}

pub fn should_compact(
    messages: &[Message],
    config: &CompactionConfig,
    workspace: Option<&Path>,
    external_pins: Option<&[usize]>,
    external_working_set_paths: Option<&[String]>,
) -> bool {
    if !config.enabled {
        return false;
    }

    // v0.8.11: hard floor enforcement. Below the floor (default 500K tokens
    // — see `MINIMUM_AUTO_COMPACTION_TOKENS`), automatic compaction is
    // refused because rewriting the prefix kills V4's prefix cache for
    // little budget recovery. Manual `/compact` and the `compact_now` tool
    // bypass this floor by going through different code paths.
    if config.auto_floor_tokens > 0 {
        let total_session_tokens: usize = messages
            .iter()
            .map(|m| estimate_tokens_for_message(m, false))
            .sum();
        if total_session_tokens < config.auto_floor_tokens {
            return false;
        }
    }

    let plan = plan_compaction(
        messages,
        workspace,
        KEEP_RECENT_MESSAGES,
        external_pins,
        external_working_set_paths,
    );
    let pinned_tokens: usize = plan
        .pinned_indices
        .iter()
        .map(|&idx| estimate_tokens_for_message(&messages[idx], false))
        .sum();

    let token_estimate: usize = plan
        .summarize_indices
        .iter()
        .map(|&idx| estimate_tokens_for_message(&messages[idx], false))
        .sum();
    let message_count = plan.summarize_indices.len();

    // Pinned messages consume part of the budget, so compact earlier when needed.
    let effective_token_threshold = config.token_threshold.saturating_sub(pinned_tokens);

    // Token-only trigger (v0.8.11): the prior message-count branch was a
    // 128K-era heuristic that fired compaction on long chats of small
    // messages — exactly the case where rewriting the V4 prefix cache is
    // most wasteful. Token budget is the only signal that maps to actual
    // model context pressure.
    if effective_token_threshold == 0 {
        return message_count >= MIN_SUMMARIZE_MESSAGES;
    }
    if message_count < MIN_SUMMARIZE_MESSAGES {
        return false;
    }
    token_estimate > effective_token_threshold
}
