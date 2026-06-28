//! P3: compaction summary in messages layer (`[COMPACTED_HISTORY]`) vs system Frankenstein.

use crate::models::{ContentBlock, Message, SystemBlock, SystemPrompt};
use zagens_core::engine::COMPACTED_HISTORY_MARKER;

/// True when any text block in the message carries the compacted-history marker.
#[must_use]
pub fn message_is_compacted_history(message: &Message) -> bool {
    message.content.iter().any(|block| match block {
        ContentBlock::Text { text, .. } => text.contains(COMPACTED_HISTORY_MARKER),
        _ => false,
    })
}

/// Extract the body of the most recent `[COMPACTED_HISTORY]` user message, if any.
#[must_use]
pub fn extract_compacted_history_text(messages: &[Message]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|m| m.role == "user" && message_is_compacted_history(m))
        .and_then(|m| {
            m.content.iter().find_map(|block| match block {
                ContentBlock::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
        })
}

#[must_use]
pub fn build_compaction_system_pointer() -> SystemPrompt {
    SystemPrompt::Text(format!(
        "{COMPACTED_HISTORY_MARKER}: archived conversation summary lives in the message \
         transcript (read-only). Prefer pinned and recent messages for verbatim detail."
    ))
}

#[must_use]
pub fn build_compacted_history_message(
    summary_safe: &str,
    workflow_safe: &str,
    anchors_section: &str,
    manual: bool,
) -> Message {
    let header = if manual {
        "## Compacted history (manual /compact — reversible via artifact)"
    } else {
        "## Compacted history (auto compaction)"
    };
    Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text: format!(
                "{COMPACTED_HISTORY_MARKER}\n\n\
                 {header}\n\n\
                 {anchors_section}\
                 <compaction_summary>\n{summary_safe}\n</compaction_summary>\n\n\
                 ---\n\n\
                 ## Workflow Context\n\n\
                 <workflow_context>\n{workflow_safe}\n</workflow_context>\n\n\
                 ---\n\n\
                 Review this block for prior decisions and paths; continue from pinned messages below."
            ),
            cache_control: None,
        }],
    }
}

#[must_use]
pub fn build_legacy_compaction_summary_prompt(
    summary_safe: &str,
    workflow_safe: &str,
    anchors_section: &str,
    cache_summary: bool,
) -> SystemPrompt {
    let summary_block = SystemBlock {
        block_type: "text".to_string(),
        text: format!(
            "{anchors_section}\
             ## 📋 Conversation Summary (Auto-Generated)\n\n\
             <compaction_summary>\n{summary_safe}\n</compaction_summary>\n\n\
             ---\n\n\
             ## 🔍 Workflow Context\n\n\
             <workflow_context>\n{workflow_safe}\n</workflow_context>\n\n\
             ---\n\n\
             ## 💡 What to Do Next\n\n\
             You have just resumed from a context compaction. The conversation above was summarized to save space. \
             Review the summary and workflow context, then continue helping the user with their task. \
             If you need more details about the summarized portion, ask the user to clarify.\n\n\
             ---\n\n\
             Pinned messages follow:"
        ),
        cache_control: if cache_summary {
            Some(crate::models::CacheControl {
                cache_type: "ephemeral".to_string(),
            })
        } else {
            None
        },
    };
    SystemPrompt::Blocks(vec![summary_block])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compacted_history_message_carries_marker() {
        let msg = build_compacted_history_message("sum", "flow", "", true);
        assert!(message_is_compacted_history(&msg));
        let text = extract_compacted_history_text(std::slice::from_ref(&msg)).expect("text");
        assert!(text.contains("sum"));
    }

    #[test]
    fn system_pointer_is_short_and_marked() {
        let prompt = build_compaction_system_pointer();
        let SystemPrompt::Text(t) = prompt else {
            panic!("expected text prompt");
        };
        assert!(t.contains(COMPACTED_HISTORY_MARKER));
        assert!(t.len() < 400);
    }
}
