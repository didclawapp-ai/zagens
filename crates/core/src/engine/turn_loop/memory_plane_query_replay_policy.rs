//! Memory-plane query replay cross-check (Phase 3b batch 4 / 8e).

use crate::engine::kernel_event::KernelEvent;
use crate::engine::turn_machine::{TurnKernelProjection, replay_effect_counts};

/// Count `MemoryPlaneQueried` rows in a turn log.
#[must_use]
pub fn count_memory_plane_queried_events(events: &[KernelEvent]) -> u32 {
    events
        .iter()
        .filter(|event| matches!(event, KernelEvent::MemoryPlaneQueried { .. }))
        .count() as u32
}

/// Verify logged query events match replay-derived `QueryMemory` effect counts.
#[must_use]
pub fn verify_memory_plane_query_replay_coherence(events: &[KernelEvent]) -> Option<String> {
    let logged = count_memory_plane_queried_events(events);
    if logged == 0 {
        return None;
    }
    let replayed = replay_effect_counts(events).query_memory;
    if logged == replayed {
        None
    } else {
        Some(format!(
            "memory_plane_queried log={logged} replay_query_memory={replayed}"
        ))
    }
}

/// Verify projection query counter matches the event log when queries were logged.
#[must_use]
pub fn verify_memory_plane_query_projection_coherence(events: &[KernelEvent]) -> Option<String> {
    let logged = count_memory_plane_queried_events(events);
    if logged == 0 {
        return None;
    }
    let projection = TurnKernelProjection::from_events(events);
    if projection.memory_plane_query_count == logged {
        None
    } else {
        Some(format!(
            "memory_plane_query_count proj={} log={logged}",
            projection.memory_plane_query_count
        ))
    }
}

/// Per-step anchor: logged `MemoryPlaneQueried` rows vs replay `QueryMemory` effects.
#[must_use]
pub fn verify_step_query_memory_anchor(
    turn_events: &[KernelEvent],
    step_idx: u32,
) -> Option<String> {
    let logged = turn_events
        .iter()
        .filter(|event| {
            matches!(
                event,
                KernelEvent::MemoryPlaneQueried {
                    step_idx: s,
                    ..
                } if *s == step_idx
            )
        })
        .count();
    let replayed = replay_query_memory_count_at_step(turn_events, step_idx) as usize;
    if logged == 0 && replayed == 0 {
        return None;
    }
    if logged == replayed {
        None
    } else {
        Some(format!(
            "step {step_idx} MemoryPlaneQueried log={logged} replay_query_memory={replayed}"
        ))
    }
}

/// Count replay-derived `QueryMemory` effects immediately before a step's `ModelRequestIssued`.
#[must_use]
pub fn replay_query_memory_count_at_step(turn_events: &[KernelEvent], step_idx: u32) -> u32 {
    let mut projection = TurnKernelProjection::default();
    let mut planned: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    let mut count = 0u32;
    for event in turn_events {
        if let KernelEvent::ToolCallPlanned {
            call_id,
            tool_name,
            input_json,
            ..
        } = event
        {
            planned.insert(call_id.clone(), (tool_name.clone(), input_json.clone()));
        }
        if let KernelEvent::ToolCallFinished {
            call_id, outcome, ..
        } = event
        {
            crate::engine::turn_loop::memory_plane_working_policy::record_working_set_path_touch(
                &mut projection,
                &planned,
                call_id,
                outcome,
            );
            planned.remove(call_id);
        }
        if let KernelEvent::ModelRequestIssued {
            step_idx: event_step,
            ..
        } = event
        {
            if *event_step == step_idx {
                count = crate::engine::turn_loop::memory_plane_query_policy::query_memory_effects_before_model_call(
                    &projection,
                    None,
                )
                .len() as u32;
            }
        }
        projection.apply(event);
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::kernel_event::{KernelEvent, TurnOutcome};
    use crate::engine::turn_machine::{Effect, replay_effect_counts, replay_turn_effects};
    use crate::turn::TurnLoopMode;

    #[test]
    fn skips_when_no_logged_queries() {
        let events = vec![KernelEvent::TurnStarted {
            turn_id: "t1".into(),
            mode: TurnLoopMode::Agent,
            input_text: "x".into(),
            max_steps: 3,
        }];
        assert!(verify_memory_plane_query_replay_coherence(&events).is_none());
    }

    #[test]
    fn projection_counts_logged_queries() {
        let events = vec![
            KernelEvent::MemoryPlaneQueried {
                turn_id: "t1".into(),
                step_idx: 2,
                layer: "working".into(),
                query_key: "working_set".into(),
                compiler_source: "working_set".into(),
            },
            KernelEvent::MemoryPlaneQueried {
                turn_id: "t1".into(),
                step_idx: 2,
                layer: "archival".into(),
                query_key: "compaction_summary".into(),
                compiler_source: "memory.compaction".into(),
            },
        ];
        let p = TurnKernelProjection::from_events(&events);
        assert_eq!(p.memory_plane_query_count, 2);
        assert!(verify_memory_plane_query_projection_coherence(&events).is_none());
    }

    #[test]
    fn replay_coherence_skips_scratchpad_fixture_without_logged_queries() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/scratchpad_compaction.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        assert!(verify_memory_plane_query_replay_coherence(&events).is_none());
        assert_eq!(replay_effect_counts(&events).query_memory, 0);
    }

    #[test]
    fn step_anchor_matches_replay_on_second_model_request() {
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
            KernelEvent::MemoryPlaneQueried {
                turn_id: "t1".into(),
                step_idx: 2,
                layer: "working".into(),
                query_key: "working_set".into(),
                compiler_source: "working_set".into(),
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 2,
            },
        ];
        assert!(verify_step_query_memory_anchor(&events, 2).is_none());
        assert_eq!(replay_query_memory_count_at_step(&events, 2), 1);
        let all_effects = replay_turn_effects(&events);
        assert_eq!(
            all_effects
                .iter()
                .filter(|e| matches!(e, Effect::QueryMemory { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn replay_coherence_detects_logged_vs_replay_mismatch() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 3,
            },
            KernelEvent::MemoryPlaneQueried {
                turn_id: "t1".into(),
                step_idx: 1,
                layer: "working".into(),
                query_key: "scratchpad_summary".into(),
                compiler_source: String::new(),
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        assert!(verify_memory_plane_query_replay_coherence(&events).is_some());
    }
}
