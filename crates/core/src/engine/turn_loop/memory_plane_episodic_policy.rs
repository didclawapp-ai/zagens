//! Episodic / TopicMemory query derivation (Phase 3b batch 4 / 8e–8g).

use crate::engine::turn_machine::{Effect, TurnKernelProjection};

use super::memory_plane_projection_policy::MemoryPlaneLayer;

/// Symbolic query key for topic-memory episodic reads.
pub const QUERY_TOPIC_EPISODIC: &str = "topic_episodic";

/// Optional live hints when episodic kernel events are not yet in the shadow log.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryPlaneEpisodicHints {
    pub topic_memory_enabled: bool,
}

/// Whether episodic material is present for a pre-`CallModel` query.
#[must_use]
pub fn episodic_material_present(
    projection: &TurnKernelProjection,
    hints: Option<MemoryPlaneEpisodicHints>,
) -> bool {
    if projection.topic_memory_injection_count > 0 {
        return true;
    }
    hints.is_some_and(|h| {
        h.topic_memory_enabled && projection.step_idx > 1 && projection.model_message_count > 0
    })
}

/// Derive episodic-layer queries before `CallModel`.
#[must_use]
pub fn episodic_query_effects_before_model_call(
    projection: &TurnKernelProjection,
    hints: Option<MemoryPlaneEpisodicHints>,
) -> Vec<Effect> {
    if !episodic_material_present(projection, hints) {
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
    fn episodic_query_reserved_without_material() {
        let projection = TurnKernelProjection::default();
        assert!(
            episodic_query_effects_before_model_call(
                &projection,
                Some(MemoryPlaneEpisodicHints {
                    topic_memory_enabled: true,
                })
            )
            .is_empty()
        );
    }

    #[test]
    fn episodic_query_from_topic_memory_injected_event() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 5,
            },
            KernelEvent::TopicMemoryInjected {
                turn_id: "t1".into(),
                step_idx: 2,
                block_token_est: 128,
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
        let effects = episodic_query_effects_before_model_call(&projection, None);
        assert_eq!(effects.len(), 1);
        assert!(matches!(
            &effects[0],
            Effect::QueryMemory {
                layer: MemoryPlaneLayer::Episodic,
                query_key,
            } if query_key == QUERY_TOPIC_EPISODIC
        ));
    }

    #[test]
    fn episodic_query_emits_on_step_two_when_enabled_via_hints() {
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
                text_preview: String::new(),
                assistant_text: String::new(),
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
            Some(MemoryPlaneEpisodicHints {
                topic_memory_enabled: true,
            }),
        );
        assert_eq!(effects.len(), 1);
    }
}
