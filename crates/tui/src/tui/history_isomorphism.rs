//! User-visible transcript isomorphism between API messages and TUI history (A1.4).
//!
//! `apply_loaded_session` rebuilds `App.history` via [`history_cells_from_message`].
//! These helpers assert that user/assistant (and tool-result bodies) stay aligned
//! after compaction, trim, and JSONL reconstruct paths.

use crate::models::{ContentBlock, Message};
use crate::tui::history::{history_cells_from_message, HistoryCell};

/// Rebuild renderable history cells from persisted/API messages (session load path).
#[must_use]
pub fn rebuild_history_from_messages(messages: &[Message]) -> Vec<HistoryCell> {
    messages
        .iter()
        .flat_map(history_cells_from_message)
        .collect()
}

/// User/assistant text blocks in transcript order (matches `reconstruct_messages` subset).
#[must_use]
pub fn user_assistant_texts_from_messages(messages: &[Message]) -> Vec<String> {
    user_assistant_texts_from_history(&rebuild_history_from_messages(messages))
}

/// User/assistant text extracted from history cells.
#[must_use]
pub fn user_assistant_texts_from_history(cells: &[HistoryCell]) -> Vec<String> {
    cells
        .iter()
        .filter_map(|cell| match cell {
            HistoryCell::User { content } => Some(content.clone()),
            HistoryCell::Assistant { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect()
}

/// Tool-result bodies from API messages (includes routed `[workshop-ref: …]` synthesis).
#[must_use]
pub fn tool_result_bodies_from_messages(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .flat_map(|msg| {
            msg.content.iter().filter_map(|block| match block {
                ContentBlock::ToolResult { content, .. } => Some(content.clone()),
                _ => None,
            })
        })
        .collect()
}

/// Thinking block text in transcript order from API messages.
#[must_use]
pub fn thinking_texts_from_messages(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .flat_map(|msg| {
            msg.content.iter().filter_map(|block| match block {
                ContentBlock::Thinking { thinking } => Some(thinking.clone()),
                _ => None,
            })
        })
        .collect()
}

/// Thinking text from rebuilt history cells.
#[must_use]
pub fn thinking_texts_from_history(cells: &[HistoryCell]) -> Vec<String> {
    cells
        .iter()
        .filter_map(|cell| match cell {
            HistoryCell::Thinking { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect()
}

/// Whether rebuilt history user/assistant text matches the source messages.
#[must_use]
pub fn history_user_assistant_matches_messages(messages: &[Message]) -> bool {
    user_assistant_texts_from_messages(messages)
        == user_assistant_texts_from_history(&rebuild_history_from_messages(messages))
}

/// Whether thinking blocks round-trip through [`history_cells_from_message`].
#[must_use]
pub fn history_thinking_matches_messages(messages: &[Message]) -> bool {
    let cells = rebuild_history_from_messages(messages);
    thinking_texts_from_messages(messages) == thinking_texts_from_history(&cells)
}

/// Core transcript isomorphism: user/assistant + thinking (A1.4).
///
/// Tool *cells* are built from live turn state, not persisted `Message` alone;
/// tool-result *bodies* in messages are checked separately via
/// [`tool_result_bodies_from_messages`].
#[must_use]
pub fn history_transcript_core_matches_messages(messages: &[Message]) -> bool {
    history_user_assistant_matches_messages(messages) && history_thinking_matches_messages(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deepseek_core::context_partition::message_has_external_ref;

    fn msg(role: &str, text: &str) -> Message {
        Message {
            role: role.to_string(),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
                cache_control: None,
            }],
        }
    }

    fn tool_result(content: &str) -> Message {
        Message {
            role: "tool".to_string(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "toolu_test".to_string(),
                content: content.to_string(),
                is_error: None,
                content_blocks: None,
            }],
        }
    }

    #[test]
    fn rebuild_history_matches_user_assistant_messages() {
        let messages = vec![
            msg("user", "hello"),
            msg("assistant", "world"),
            msg("user", "follow-up"),
        ];
        assert!(history_user_assistant_matches_messages(&messages));
        assert_eq!(
            user_assistant_texts_from_messages(&messages),
            vec![
                "hello".to_string(),
                "world".to_string(),
                "follow-up".to_string()
            ]
        );
    }

    #[test]
    fn tool_result_bodies_extract_workshop_ref() {
        let ref_body = "[workshop-ref: {\"ref_id\":\"lout_hist_iso\"}]\n\nsummary body";
        let messages = vec![tool_result(ref_body)];
        let bodies = tool_result_bodies_from_messages(&messages);
        assert_eq!(bodies.len(), 1);
        assert!(message_has_external_ref(&bodies[0]));
    }

    fn msg_with_thinking(role: &str, thinking: &str) -> Message {
        Message {
            role: role.to_string(),
            content: vec![ContentBlock::Thinking {
                thinking: thinking.to_string(),
            }],
        }
    }

    #[test]
    fn rebuild_history_matches_thinking_blocks() {
        let messages = vec![
            msg("user", "question"),
            msg_with_thinking("assistant", "step one"),
            msg_with_thinking("assistant", "step two"),
            msg("assistant", "answer"),
        ];
        assert!(history_thinking_matches_messages(&messages));
        assert!(history_transcript_core_matches_messages(&messages));
        assert_eq!(
            thinking_texts_from_messages(&messages),
            vec!["step one".to_string(), "step two".to_string()]
        );
    }

    #[test]
    fn transcript_core_matches_after_mixed_blocks() {
        let messages = vec![
            msg("user", "hi"),
            msg_with_thinking("assistant", "reasoning"),
            tool_result("tool output body"),
            msg("assistant", "done"),
        ];
        assert!(history_transcript_core_matches_messages(&messages));
        assert_eq!(
            tool_result_bodies_from_messages(&messages),
            vec!["tool output body".to_string()]
        );
    }
}
