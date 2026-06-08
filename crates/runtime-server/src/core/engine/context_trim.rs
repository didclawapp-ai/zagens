//! Partition-aware emergency message trim (A1-full).

use zagens_core::chat::SystemPrompt;
use zagens_core::context_partition::{SessionContextPartition, next_message_index_to_trim};
use zagens_core::engine::context::{
    MIN_RECENT_MESSAGES_TO_KEEP, count_oldest_messages_to_drain, estimate_input_tokens_conservative,
};

use crate::compaction::{KEEP_RECENT_MESSAGES, plan_compaction};
use crate::core::engine::scratchpad_flow;
use crate::models::Message as TuiMessage;
use std::path::Path;
use zagens_core::working_set::WorkingSet;

/// Build the session hot/cold partition using the same heuristics as compaction.
#[must_use]
pub fn session_context_partition_for_trim(
    messages: &[TuiMessage],
    workspace: &Path,
    working_set: &WorkingSet,
    scratchpad_run_id: Option<&str>,
) -> SessionContextPartition {
    let pins = working_set.pinned_message_indices(messages, workspace);
    let mut paths = working_set.top_paths(24);
    scratchpad_flow::extend_compaction_paths(workspace, scratchpad_run_id, &mut paths);
    let plan = plan_compaction(
        messages,
        Some(workspace),
        KEEP_RECENT_MESSAGES,
        Some(&pins),
        Some(&paths),
    );
    plan.context_partition(messages, KEEP_RECENT_MESSAGES)
}

/// Trim `messages` toward `target_input_budget`, dropping cold-summary first and
/// preserving hot / pinned / external-ref tiers; legacy front-drain as last resort.
pub fn trim_messages_partition_aware(
    messages: &mut Vec<TuiMessage>,
    system_prompt: Option<&SystemPrompt>,
    target_input_budget: usize,
    workspace: &Path,
    working_set: &WorkingSet,
    scratchpad_run_id: Option<&str>,
) -> usize {
    let before = messages.len();

    // Fast path when far over budget: bulk-drain avoids O(n²) single-message
    // removal with full partition recompute on each step (capacity guardrail tests).
    if estimate_input_tokens_conservative(messages, system_prompt) > target_input_budget {
        let drain = count_oldest_messages_to_drain(messages, system_prompt, target_input_budget);
        if drain > 0 {
            messages.drain(0..drain);
        }
    }

    while estimate_input_tokens_conservative(messages, system_prompt) > target_input_budget
        && messages.len() > MIN_RECENT_MESSAGES_TO_KEEP
    {
        let partition =
            session_context_partition_for_trim(messages, workspace, working_set, scratchpad_run_id);
        let Some(idx) = next_message_index_to_trim(&partition, messages.len()) else {
            break;
        };
        messages.remove(idx);
    }

    if estimate_input_tokens_conservative(messages, system_prompt) > target_input_budget {
        let drain = count_oldest_messages_to_drain(messages, system_prompt, target_input_budget);
        if drain > 0 {
            messages.drain(0..drain);
        }
    }

    before.saturating_sub(messages.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use zagens_core::chat::ContentBlock;
    use zagens_core::context_partition::message_has_external_ref;
    use zagens_core::working_set::WorkingSet;

    fn msg(role: &str, text: &str) -> TuiMessage {
        TuiMessage {
            role: role.to_string(),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
                cache_control: None,
            }],
        }
    }

    fn message_text(message: &TuiMessage) -> String {
        message
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn trim_preserves_workshop_ref_message() {
        let dir = tempdir().expect("tempdir");
        let workspace = dir.path();
        let working_set = WorkingSet::default();
        let ref_body = "[workshop-ref: {\"ref_id\":\"lout_trim_test\"}]\n[workshop-synthesis: tool=read_file]\n\nsummary";
        let mut messages: Vec<TuiMessage> = (0..8)
            .map(|i| msg("user", &format!("filler-{i}-{}", "z".repeat(4000))))
            .chain(std::iter::once(msg("tool", ref_body)))
            .chain((0..4).map(|i| msg("user", &format!("recent-{i}"))))
            .collect();

        let removed =
            trim_messages_partition_aware(&mut messages, None, 500, workspace, &working_set, None);
        assert!(removed > 0, "should trim cold filler");
        assert!(
            messages
                .iter()
                .any(|m| message_has_external_ref(&message_text(m))),
            "workshop-ref message must survive partition-aware trim"
        );
        assert!(
            crate::transcript_isomorphism::history_transcript_core_matches_messages(&messages),
            "trimmed messages must round-trip through TUI history rebuild (A1.4)"
        );
    }

    #[test]
    fn trim_preserves_thinking_transcript_isomorphism() {
        use zagens_core::chat::ContentBlock;

        let dir = tempdir().expect("tempdir");
        let workspace = dir.path();
        let working_set = WorkingSet::default();
        let mut messages: Vec<TuiMessage> = (0..6)
            .map(|i| msg("user", &format!("filler-{i}-{}", "z".repeat(3000))))
            .collect();
        messages.push(TuiMessage {
            role: "assistant".to_string(),
            content: vec![ContentBlock::Thinking {
                thinking: "pinned-reasoning".to_string(),
            }],
        });
        messages.push(msg("assistant", "final-answer"));
        messages.extend((0..2).map(|i| msg("user", &format!("recent-{i}"))));

        let removed =
            trim_messages_partition_aware(&mut messages, None, 800, workspace, &working_set, None);
        assert!(removed > 0);
        assert!(
            crate::transcript_isomorphism::history_transcript_core_matches_messages(&messages),
            "thinking + user/assistant must stay isomorphic after trim"
        );
    }
}
