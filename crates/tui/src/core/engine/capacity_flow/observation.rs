//! Capacity observation inputs derived from session and turn state.

use crate::models::context_window_for_model;
use crate::models::{ContentBlock, LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS};

use super::super::*;

impl Engine {
    pub(in crate::core::engine) fn capacity_observation(&self, turn: &TurnContext) -> CapacityObservationInput {
        let message_window = self.config.capacity.profile_window.max(8) * 3;
        let action_count_this_turn = usize::try_from(turn.step)
            .unwrap_or(usize::MAX)
            .saturating_add(turn.tool_calls.len())
            .saturating_add(1);
        let tool_calls_recent_window = self.recent_tool_call_count(message_window);
        let unique_reference_ids_recent_window =
            self.recent_unique_reference_count(message_window, turn);
        let context_window = usize::try_from(
            context_window_for_model(&self.session.model)
                .unwrap_or(LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS),
        )
        .unwrap_or(usize::try_from(LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS).unwrap_or(128_000))
        .max(1);
        let context_used_ratio = (self.estimated_input_tokens() as f64) / (context_window as f64);

        CapacityObservationInput {
            turn_index: self.turn_counter,
            model: self.session.model.clone(),
            action_count_this_turn,
            tool_calls_recent_window,
            unique_reference_ids_recent_window,
            context_used_ratio,
        }
    }

    pub(in crate::core::engine) fn recent_tool_call_count(&self, message_window: usize) -> usize {
        self.session
            .messages
            .iter()
            .rev()
            .take(message_window)
            .map(|msg| {
                msg.content
                    .iter()
                    .filter(|block| {
                        matches!(
                            block,
                            ContentBlock::ToolUse { .. } | ContentBlock::ToolResult { .. }
                        )
                    })
                    .count()
            })
            .sum()
    }

    pub(in crate::core::engine) fn recent_unique_reference_count(
        &self,
        message_window: usize,
        turn: &TurnContext,
    ) -> usize {
        let mut refs = std::collections::HashSet::new();
        for msg in self.session.messages.iter().rev().take(message_window) {
            for block in &msg.content {
                match block {
                    ContentBlock::ToolUse { id, .. } => {
                        refs.insert(id.clone());
                    }
                    ContentBlock::ToolResult { tool_use_id, .. } => {
                        refs.insert(tool_use_id.clone());
                    }
                    ContentBlock::Text { text, .. } => {
                        for token in text.split_whitespace() {
                            if token.contains('/') || token.contains('.') {
                                refs.insert(
                                    token
                                        .trim_matches(|c: char| ",.;:()[]{}".contains(c))
                                        .to_string(),
                                );
                            }
                        }
                    }
                    ContentBlock::Thinking { .. }
                    | ContentBlock::ServerToolUse { .. }
                    | ContentBlock::ToolSearchToolResult { .. }
                    | ContentBlock::CodeExecutionToolResult { .. } => {}
                }
            }
        }
        for tool_call in turn.tool_calls.iter().rev().take(8) {
            refs.insert(tool_call.id.clone());
        }
        for path in self.session.working_set.top_paths(8) {
            refs.insert(path);
        }
        refs.retain(|item| !item.is_empty());
        refs.len()
    }
}
