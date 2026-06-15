//! ContextCompiler source mapping for memory-plane queries (Phase 3b batch 4 / 8e).

use super::memory_plane_episodic_policy::QUERY_TOPIC_EPISODIC;
use super::memory_plane_query_policy::{
    QUERY_COMPACTION_SUMMARY, QUERY_SCRATCHPAD_REMINDER, QUERY_SCRATCHPAD_SUMMARY,
    QUERY_WORKING_SET,
};

/// Map a symbolic memory query key to a ContextCompiler source id.
#[must_use]
pub fn compiler_source_for_query_key(query_key: &str) -> &'static str {
    match query_key {
        QUERY_SCRATCHPAD_SUMMARY => "memory.scratchpad_summary",
        QUERY_SCRATCHPAD_REMINDER => "memory.scratchpad_reminder",
        QUERY_COMPACTION_SUMMARY => "memory.compaction",
        QUERY_WORKING_SET => "working_set",
        QUERY_TOPIC_EPISODIC => "topic_memory",
        _ => "memory.unknown",
    }
}

/// Whether the projection already carries material for this query key (replay substrate).
#[must_use]
pub fn query_key_has_projection_material(
    projection: &crate::engine::turn_machine::TurnKernelProjection,
    query_key: &str,
) -> bool {
    match query_key {
        QUERY_SCRATCHPAD_SUMMARY => projection.scratchpad_summary_injected,
        QUERY_SCRATCHPAD_REMINDER => projection.scratchpad_reminder_count > 0,
        QUERY_COMPACTION_SUMMARY => projection.compaction_artifact_count > 0,
        QUERY_WORKING_SET => projection.working_set_path_touch_count > 0,
        QUERY_TOPIC_EPISODIC => projection.step_idx > 1 && projection.model_message_count > 0,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_query_keys() {
        assert_eq!(
            compiler_source_for_query_key(QUERY_WORKING_SET),
            "working_set"
        );
        assert_eq!(
            compiler_source_for_query_key(QUERY_TOPIC_EPISODIC),
            "topic_memory"
        );
    }

    #[test]
    fn projection_material_gate() {
        let mut projection = crate::engine::turn_machine::TurnKernelProjection::default();
        projection.working_set_path_touch_count = 2;
        assert!(query_key_has_projection_material(
            &projection,
            QUERY_WORKING_SET
        ));
        assert!(!query_key_has_projection_material(
            &projection,
            QUERY_COMPACTION_SUMMARY
        ));
    }
}
