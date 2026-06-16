//! Memory Plane read-side query derivation (Phase 3b batch 4 / 8c).

use crate::engine::turn_machine::{Effect, TurnKernelProjection};

use super::memory_plane_episodic_policy::{
    MemoryPlaneEpisodicHints, episodic_query_effects_before_model_call,
};
use super::memory_plane_projection_policy::MemoryPlaneLayer;

/// Target memory plane layer for a read query (alias of projection taxonomy).
pub type MemoryPlaneQueryLayer = MemoryPlaneLayer;

/// Symbolic query keys for working-layer reads.
pub const QUERY_SCRATCHPAD_SUMMARY: &str = "scratchpad_summary";
pub const QUERY_SCRATCHPAD_REMINDER: &str = "scratchpad_reminder";
/// Symbolic query key for archival compaction summary reads.
pub const QUERY_COMPACTION_SUMMARY: &str = "compaction_summary";
/// Symbolic query key for working-set path substrate reads.
pub const QUERY_WORKING_SET: &str = "working_set";
/// Symbolic query key for user-memory block reads during system prompt refresh.
pub const QUERY_USER_MEMORY: &str = "user_memory";

/// Derive pre-`CallModel` memory queries from the current projection (pure, no IO).
#[must_use]
pub fn query_memory_effects_before_model_call(
    projection: &TurnKernelProjection,
    episodic_hints: Option<MemoryPlaneEpisodicHints>,
) -> Vec<Effect> {
    let mut out = Vec::new();
    if projection.scratchpad_summary_injected {
        out.push(Effect::QueryMemory {
            layer: MemoryPlaneQueryLayer::Working,
            query_key: QUERY_SCRATCHPAD_SUMMARY.into(),
        });
    }
    if projection.scratchpad_reminder_count > 0 {
        out.push(Effect::QueryMemory {
            layer: MemoryPlaneQueryLayer::Working,
            query_key: QUERY_SCRATCHPAD_REMINDER.into(),
        });
    }
    if projection.compaction_artifact_count > 0 {
        out.push(Effect::QueryMemory {
            layer: MemoryPlaneQueryLayer::Archival,
            query_key: QUERY_COMPACTION_SUMMARY.into(),
        });
    }
    if projection.working_set_path_touch_count > 0 {
        out.push(Effect::QueryMemory {
            layer: MemoryPlaneQueryLayer::Working,
            query_key: QUERY_WORKING_SET.into(),
        });
    }
    out.extend(episodic_query_effects_before_model_call(
        projection,
        episodic_hints,
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::kernel_event::{KernelEvent, MessageRange, TurnOutcome};
    use crate::turn::TurnLoopMode;

    #[test]
    fn no_queries_on_empty_projection() {
        let projection = TurnKernelProjection::default();
        assert!(query_memory_effects_before_model_call(&projection, None).is_empty());
    }

    #[test]
    fn replay_emits_queries_before_call_model_when_material_present() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 5,
            },
            KernelEvent::ScratchpadSummaryInjected {
                turn_id: "t1".into(),
                at_step: 1,
            },
            KernelEvent::CompactionArtifactCreated {
                turn_id: "t1".into(),
                artifact_id: "art-1".into(),
                replaced_range: MessageRange { from: 0, to: 3 },
                summary_token_count: 64,
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 2,
                request_fp: crate::engine::request_fingerprint::RequestFingerprint {
                    static_prefix_sha256: "aa".into(),
                    full_prefix_sha256: "bb".into(),
                },
                token_budget: 4096,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 2,
            },
        ];
        let effects = crate::engine::turn_machine::replay_turn_effects(&events);
        let query_count = effects
            .iter()
            .filter(|e| matches!(e, Effect::QueryMemory { .. }))
            .count();
        assert_eq!(query_count, 2);
        let call_idx = effects
            .iter()
            .position(|e| matches!(e, Effect::CallModel { .. }))
            .expect("call model");
        assert!(call_idx >= 2);
        assert!(matches!(
            &effects[call_idx - 2],
            Effect::QueryMemory {
                layer: MemoryPlaneQueryLayer::Working,
                query_key,
            } if query_key == QUERY_SCRATCHPAD_SUMMARY
        ));
        assert!(matches!(
            &effects[call_idx - 1],
            Effect::QueryMemory {
                layer: MemoryPlaneQueryLayer::Archival,
                ..
            }
        ));
    }

    #[test]
    fn replay_emits_working_set_query_before_second_model_call() {
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
                request_fp: crate::engine::request_fingerprint::RequestFingerprint {
                    static_prefix_sha256: "aa".into(),
                    full_prefix_sha256: "bb".into(),
                },
                token_budget: 4096,
            },
            KernelEvent::ToolCallPlanned {
                turn_id: "t1".into(),
                step_idx: 1,
                call_id: "c1".into(),
                tool_name: "list_dir".into(),
                input_json: r#"{"path":"src/"}"#.into(),
                decision: crate::engine::kernel_event::PolicyDecision::new(false, true, true),
            },
            KernelEvent::ToolCallFinished {
                turn_id: "t1".into(),
                call_id: "c1".into(),
                tool_name: "list_dir".into(),
                outcome: crate::engine::kernel_event::ToolOutcome::Success,
                duration_ms: 1,
                wrote_state: false,
                result_preview: String::new(),
                session_content: String::new(),
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 2,
                request_fp: crate::engine::request_fingerprint::RequestFingerprint {
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
        let effects = crate::engine::turn_machine::replay_turn_effects(&events);
        let working_idx = effects.iter().position(|e| {
            matches!(
                e,
                Effect::QueryMemory {
                    query_key,
                    layer: MemoryPlaneQueryLayer::Working,
                    ..
                } if query_key == QUERY_WORKING_SET
            )
        });
        assert!(working_idx.is_some());
        let call_idx = effects
            .iter()
            .rposition(|e| matches!(e, Effect::CallModel { .. }))
            .expect("second call model");
        assert_eq!(working_idx.unwrap() + 1, call_idx);
    }
}
