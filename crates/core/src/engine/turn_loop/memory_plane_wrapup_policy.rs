//! Memory Plane batch-4 wrap-up gate (Phase 3b batch 4 / 8h).

use crate::engine::kernel_event::KernelEvent;

/// Unified batch-4 memory-plane replay gate for golden fixtures and CI replay verify.
#[must_use]
pub fn verify_memory_plane_batch4_coherence(events: &[KernelEvent]) -> Option<String> {
    let mut diffs = Vec::new();
    if let Some(summary) = crate::engine::turn_machine::verify_memory_projection_chain(events) {
        diffs.push(summary);
    }
    if let Some(summary) =
        crate::engine::turn_loop::memory_plane_query_replay_policy::verify_memory_plane_query_replay_coherence(
            events,
        )
    {
        diffs.push(format!("query_replay: {summary}"));
    }
    if let Some(summary) =
        crate::engine::turn_loop::memory_plane_compiler_policy::verify_memory_plane_compiler_source_coherence(
            events,
        )
    {
        diffs.push(format!("compiler_source: {summary}"));
    }
    if let Some(summary) =
        crate::engine::turn_loop::memory_plane_compiler_policy::verify_compiler_queried_sources_coherence(
            events,
        )
    {
        diffs.push(format!("compiler_projection: {summary}"));
    }
    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_plane_query_fixture_passes_batch4_gate() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/memory_plane_query.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        assert!(
            verify_memory_plane_batch4_coherence(&events).is_none(),
            "memory_plane_query.json batch4 gate failed"
        );
        let p = crate::engine::turn_machine::TurnKernelProjection::from_events(&events);
        assert_eq!(p.topic_memory_injection_count, 1);
        assert_eq!(p.memory_plane_query_count, 2);
        assert_eq!(p.working_set_path_touch_count, 1);
    }
}
