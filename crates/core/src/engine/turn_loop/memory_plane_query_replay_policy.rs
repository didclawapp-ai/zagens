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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::kernel_event::{KernelEvent, TurnOutcome};
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
