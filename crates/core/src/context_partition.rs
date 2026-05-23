//! Hot-window / cold-zone context partition (A1.1).
//!
//! Classifies in-session messages for compaction and externalized tool output:
//! - **Hot window:** recent tail kept verbatim in API context.
//! - **Cold zone:** older messages summarized, pinned, or represented by external refs.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::chat::{ContentBlock, Message};

/// Marker embedded in routed large tool output (`large_output_router`).
pub const WORKSHOP_REF_MARKER: &str = "[workshop-ref:";

/// Per-message tier within the session context partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageContextTier {
    /// Trailing messages kept verbatim (hot window).
    Hot,
    /// Outside the hot tail but pinned (errors, patches, working-set paths).
    Pinned,
    /// Cold zone — eligible for compaction summary.
    ColdSummary,
    /// Cold zone — body holds an external/workshop ref; raw bytes may be off-disk.
    ColdExternalRef,
}

/// Indices and config for the hot (recent) window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotWindow {
    pub keep_recent_count: usize,
    pub message_indices: Vec<usize>,
}

/// Indices in the cold zone (everything not in the hot tail).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColdZone {
    pub summary_candidate_indices: Vec<usize>,
    pub pinned_indices: Vec<usize>,
    pub external_ref_indices: Vec<usize>,
}

/// Full partition view over `messages.len()` indices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionContextPartition {
    pub tiers: Vec<MessageContextTier>,
    pub hot: HotWindow,
    pub cold: ColdZone,
}

/// Whether visible message text carries a workshop / large-output external ref.
#[must_use]
pub fn message_has_external_ref(text: &str) -> bool {
    text.contains(WORKSHOP_REF_MARKER)
}

fn message_visible_text(message: &Message) -> String {
    message
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text, .. } => Some(text.as_str()),
            ContentBlock::ToolResult { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Classify each message index into hot / cold tiers.
///
/// `pinned_indices` is typically produced by TUI `plan_compaction` heuristics;
/// `keep_recent` is the hot-window tail size (same semantics as `plan_compaction`).
#[must_use]
pub fn classify_session_messages(
    messages: &[Message],
    keep_recent: usize,
    pinned_indices: &BTreeSet<usize>,
) -> SessionContextPartition {
    let len = messages.len();
    let recent_start = len.saturating_sub(keep_recent);

    let mut tiers = Vec::with_capacity(len);
    let mut hot = HotWindow {
        keep_recent_count: keep_recent,
        message_indices: Vec::new(),
    };
    let mut cold = ColdZone::default();

    for (idx, msg) in messages.iter().enumerate() {
        let text = message_visible_text(msg);
        let tier = if message_has_external_ref(&text) {
            MessageContextTier::ColdExternalRef
        } else if idx >= recent_start {
            MessageContextTier::Hot
        } else if pinned_indices.contains(&idx) {
            MessageContextTier::Pinned
        } else {
            MessageContextTier::ColdSummary
        };
        tiers.push(tier);
        match tier {
            MessageContextTier::Hot => hot.message_indices.push(idx),
            MessageContextTier::Pinned => cold.pinned_indices.push(idx),
            MessageContextTier::ColdSummary => cold.summary_candidate_indices.push(idx),
            MessageContextTier::ColdExternalRef => cold.external_ref_indices.push(idx),
        }
    }

    SessionContextPartition {
        tiers,
        hot,
        cold,
    }
}

/// Indices that must survive partition-aware emergency trim (hot + pinned + external refs).
#[must_use]
pub fn protected_message_indices(partition: &SessionContextPartition) -> BTreeSet<usize> {
    let mut set = BTreeSet::new();
    set.extend(partition.hot.message_indices.iter().copied());
    set.extend(partition.cold.pinned_indices.iter().copied());
    set.extend(partition.cold.external_ref_indices.iter().copied());
    set
}

/// Pick the next message index to drop: oldest cold-summary first, then oldest unprotected.
#[must_use]
pub fn next_message_index_to_trim(
    partition: &SessionContextPartition,
    message_count: usize,
) -> Option<usize> {
    if message_count == 0 {
        return None;
    }
    if let Some(&idx) = partition
        .cold
        .summary_candidate_indices
        .iter()
        .find(|&&idx| idx < message_count)
    {
        return Some(idx);
    }
    let protected = protected_message_indices(partition);
    (0..message_count).find(|idx| !protected.contains(idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::Message;

    fn text_message(role: &str, text: &str) -> Message {
        Message {
            role: role.to_string(),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
                cache_control: None,
            }],
        }
    }

    #[test]
    fn classify_hot_tail_and_cold_summary() {
        let messages: Vec<_> = (0..6)
            .map(|i| text_message("user", &format!("msg-{i}")))
            .collect();
        let pinned = BTreeSet::new();
        let partition = classify_session_messages(&messages, 2, &pinned);

        assert_eq!(partition.tiers.len(), 6);
        assert_eq!(partition.hot.message_indices, vec![4, 5]);
        assert_eq!(partition.cold.summary_candidate_indices, vec![0, 1, 2, 3]);
        assert!(partition.cold.pinned_indices.is_empty());
    }

    #[test]
    fn classify_pins_outside_hot_tail() {
        let messages = vec![
            text_message("user", "old patch diff --git a/foo"),
            text_message("user", "middle"),
            text_message("user", "recent-a"),
            text_message("user", "recent-b"),
        ];
        let mut pinned = BTreeSet::new();
        pinned.insert(0);
        let partition = classify_session_messages(&messages, 2, &pinned);

        assert_eq!(partition.tiers[0], MessageContextTier::Pinned);
        assert_eq!(partition.tiers[2], MessageContextTier::Hot);
        assert_eq!(partition.cold.pinned_indices, vec![0]);
    }

    #[test]
    fn classify_external_ref_overrides_hot() {
        let messages = vec![
            text_message("user", "plain"),
            text_message(
                "tool",
                "[workshop-ref: {\"ref_id\":\"lout_abcd1234\"}]\n[workshop-synthesis: tool=read_file]\n\nsummary",
            ),
        ];
        let partition = classify_session_messages(&messages, 2, &BTreeSet::new());

        assert_eq!(partition.tiers[1], MessageContextTier::ColdExternalRef);
        assert_eq!(partition.cold.external_ref_indices, vec![1]);
        assert!(message_has_external_ref(&message_visible_text(&messages[1])));
    }

    #[test]
    fn next_trim_prefers_cold_summary_before_hot() {
        let messages = vec![
            text_message("user", "cold old"),
            text_message("user", "hot recent"),
        ];
        let partition = classify_session_messages(&messages, 1, &BTreeSet::new());
        assert_eq!(
            next_message_index_to_trim(&partition, messages.len()),
            Some(0)
        );
    }

    #[test]
    fn next_trim_skips_external_ref_when_only_cold_summary_removable() {
        let messages = vec![
            text_message("user", "cold old"),
            text_message(
                "tool",
                "[workshop-ref: {\"ref_id\":\"lout_x\"}]\n\nsummary",
            ),
            text_message("user", "hot"),
        ];
        let partition = classify_session_messages(&messages, 1, &BTreeSet::new());
        assert_eq!(next_message_index_to_trim(&partition, messages.len()), Some(0));
        assert_eq!(partition.cold.external_ref_indices, vec![1]);
    }

    #[test]
    fn protected_indices_cover_hot_pin_and_external_ref() {
        let messages = vec![
            text_message("user", "cold"),
            text_message(
                "tool",
                "[workshop-ref: {\"ref_id\":\"lout_x\"}]\n\nsummary",
            ),
            text_message("user", "hot"),
        ];
        let mut pinned = BTreeSet::new();
        pinned.insert(0);
        let partition = classify_session_messages(&messages, 1, &pinned);
        let protected = protected_message_indices(&partition);
        assert!(protected.contains(&0));
        assert!(protected.contains(&1));
        assert!(protected.contains(&2));
        assert!(next_message_index_to_trim(&partition, messages.len()).is_none());
    }
}
