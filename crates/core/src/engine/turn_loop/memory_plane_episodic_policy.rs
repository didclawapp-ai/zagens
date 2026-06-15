//! Episodic / TopicMemory query derivation (Phase 3b batch 4 / 8e — reserved substrate).

use crate::engine::turn_machine::{Effect, TurnKernelProjection};

use super::memory_plane_projection_policy::MemoryPlaneLayer;

/// Symbolic query key for topic-memory episodic reads (reserved).
pub const QUERY_TOPIC_EPISODIC: &str = "topic_episodic";

/// Optional live hints not rebuildable from the kernel log alone (yet).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryPlaneEpisodicHints {
    pub topic_memory_enabled: bool,
}

/// Derive episodic-layer queries before `CallModel` when topic memory is enabled.
///
/// Reserved until dedicated TopicMemory kernel events exist; uses step/message
/// heuristics for v3 live wiring only (replay passes `topic_memory_enabled: false`).
#[must_use]
pub fn episodic_query_effects_before_model_call(
    projection: &TurnKernelProjection,
    hints: MemoryPlaneEpisodicHints,
) -> Vec<Effect> {
    if !hints.topic_memory_enabled || projection.step_idx <= 1 {
        return Vec::new();
    }
    if projection.model_message_count == 0 {
        return Vec::new();
    }
    vec![Effect::QueryMemory {
        layer: MemoryPlaneLayer::Episodic,
        query_key: QUERY_TOPIC_EPISODIC.into(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::kernel_event::{KernelEvent, TurnOutcome};
    use crate::engine::request_fingerprint::RequestFingerprint;
    use crate::turn::TurnLoopMode;

    #[test]
    fn episodic_query_reserved_until_topic_events() {
        let projection = TurnKernelProjection::default();
        assert!(
            episodic_query_effects_before_model_call(
                &projection,
                MemoryPlaneEpisodicHints {
                    topic_memory_enabled: true,
                }
            )
            .is_empty()
        );
    }

    #[test]
    fn episodic_query_emits_on_step_two_when_enabled() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 5,
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 1,
                request_fp: RequestFingerprint {
                    static_prefix_sha256: "aa".into(),
                    full_prefix_sha256: "bb".into(),
                },
                token_budget: 4096,
            },
            KernelEvent::ModelMessage {
                turn_id: "t1".into(),
                step_idx: 1,
                usage: crate::models::Usage::default(),
                block_count: 1,
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 2,
                request_fp: RequestFingerprint {
                    static_prefix_sha256: "cc".into(),
                    full_prefix_sha256: "dd".into(),
                },
                token_budget: 4096,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 2,
            },
        ];
        let projection = TurnKernelProjection::from_events(&events);
        let effects = episodic_query_effects_before_model_call(
            &projection,
            MemoryPlaneEpisodicHints {
                topic_memory_enabled: true,
            },
        );
        assert_eq!(effects.len(), 1);
        assert!(matches!(
            &effects[0],
            Effect::QueryMemory {
                layer: MemoryPlaneLayer::Episodic,
                query_key,
            } if query_key == QUERY_TOPIC_EPISODIC
        ));
    }
}
