use std::path::Path;

use crate::models::{ContentBlock, Message, SystemPrompt};
use zagens_core::compaction::CompactionConfig;
// P2-B: delegate to the canonical TokenEstimator (single calibration authority).
// All text-level counts go through it; framing formula also sourced here.
use zagens_core::engine::token_estimate::TokenEstimator;

use super::plan::plan_compaction;
use super::{KEEP_RECENT_MESSAGES, MIN_SUMMARIZE_MESSAGES};

/// Whether a message's reasoning is replayed at inference time.
///
/// DeepSeek API rule: `reasoning_content` is only replayed for messages that
/// contain `ToolUse` blocks.  Compaction token accounting uses this to match
/// actual API billing semantics.
pub(crate) fn message_has_tool_use(message: &Message) -> bool {
    message
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolUse { .. }))
}

/// Token estimate for one message.
///
/// `include_thinking`: when `true`, `Thinking` block tokens are counted.
/// Server-side blocks (`ServerToolUse`, `ToolSearchToolResult`,
/// `CodeExecutionToolResult`) are excluded — they are never sent back to the
/// API and their token cost is zero from the API's perspective.
pub(crate) fn estimate_tokens_for_message(message: &Message, include_thinking: bool) -> usize {
    let est = TokenEstimator;
    message
        .content
        .iter()
        .map(|c| match c {
            ContentBlock::Text { text, .. } => est.estimate_text(text),
            ContentBlock::Thinking { thinking } if include_thinking => est.estimate_text(thinking),
            ContentBlock::Thinking { .. } => 0,
            // ToolUse input is a serde_json::Value; Display produces compact JSON.
            ContentBlock::ToolUse { input, .. } => est.estimate_text(&input.to_string()),
            ContentBlock::ToolResult { content, .. } => est.estimate_text(content),
            // Server-side blocks are not sent to the API; cost is zero.
            ContentBlock::ServerToolUse { .. }
            | ContentBlock::ToolSearchToolResult { .. }
            | ContentBlock::CodeExecutionToolResult { .. } => 0,
        })
        .sum::<usize>()
}

pub fn estimate_tokens(messages: &[Message]) -> usize {
    // Selective thinking: only messages with tool_use replay their reasoning.
    messages
        .iter()
        .map(|m| estimate_tokens_for_message(m, message_has_tool_use(m)))
        .sum()
}

pub(crate) fn estimate_system_tokens_conservative(system: Option<&SystemPrompt>) -> usize {
    TokenEstimator.estimate_system(system)
}

/// Conservative estimate for full request input tokens (messages + system + framing).
///
/// Delegates to [`TokenEstimator::estimate_request_input_with_selective_thinking`]
/// (P2-B canonical path) so framing formula and multiplier are consistent with
/// the context-trim path (`core/engine/context.rs`) and the compiler budget solver.
#[must_use]
pub fn estimate_input_tokens_conservative(
    messages: &[Message],
    system: Option<&SystemPrompt>,
) -> usize {
    TokenEstimator.estimate_request_input_with_selective_thinking(messages, system)
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
