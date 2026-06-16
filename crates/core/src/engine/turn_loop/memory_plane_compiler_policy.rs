//! ContextCompiler source mapping for memory-plane queries (Phase 3b batch 4 / 8e–8h).

use std::collections::BTreeSet;

use crate::engine::context_compiler::{BudgetOverride, BudgetPolicy, SourceId};

use super::memory_plane_episodic_policy::QUERY_TOPIC_EPISODIC;
use super::memory_plane_query_policy::{
    QUERY_COMPACTION_SUMMARY, QUERY_SCRATCHPAD_REMINDER, QUERY_SCRATCHPAD_SUMMARY,
    QUERY_USER_MEMORY, QUERY_WORKING_SET,
};

/// Map a symbolic memory query key to a ContextCompiler source id.
#[must_use]
pub fn compiler_source_for_query_key(query_key: &str) -> &'static str {
    match query_key {
        QUERY_SCRATCHPAD_SUMMARY => "memory.scratchpad_summary",
        QUERY_SCRATCHPAD_REMINDER => "memory.scratchpad_reminder",
        QUERY_COMPACTION_SUMMARY => "memory.compaction",
        QUERY_WORKING_SET => "working_set",
        QUERY_USER_MEMORY => "memory.user",
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
        QUERY_TOPIC_EPISODIC => projection.topic_memory_injection_count > 0,
        QUERY_USER_MEMORY => false,
        _ => false,
    }
}

/// Apply post-compile force-include for sources explicitly queried this step (batch 8h).
#[must_use]
pub fn resolved_compiler_includes_for_queried_sources(
    queried: &BTreeSet<String>,
    has_compaction_from_compile: bool,
    has_working_set_from_compile: bool,
    compaction_text_nonempty: bool,
    working_set_text_nonempty: bool,
) -> (bool, bool) {
    let mut has_compaction = has_compaction_from_compile;
    let mut has_working_set = has_working_set_from_compile;
    if queried.contains("memory.compaction") && compaction_text_nonempty {
        has_compaction = true;
    }
    if queried.contains("working_set") && working_set_text_nonempty {
        has_working_set = true;
    }
    (has_compaction, has_working_set)
}

/// Budget overrides that resist eviction for queried compiler sources during overflow recompile.
#[must_use]
pub fn compiler_budget_overrides_for_queried_sources(
    queried: &BTreeSet<String>,
) -> Vec<BudgetOverride> {
    let mut out = Vec::new();
    if queried.contains("working_set") {
        out.push(BudgetOverride {
            source_id: SourceId("working_set".into()),
            new_budget: BudgetPolicy::Elastic {
                min: 800,
                max: 1500,
            },
        });
    }
    if queried.contains("memory.compaction") {
        out.push(BudgetOverride {
            source_id: SourceId("memory.compaction".into()),
            new_budget: BudgetPolicy::Elastic {
                min: 512,
                max: 4000,
            },
        });
    }
    out
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
            compiler_source_for_query_key(QUERY_USER_MEMORY),
            "memory.user"
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

    #[test]
    fn force_include_queried_working_set() {
        let queried = std::collections::BTreeSet::from(["working_set".to_string()]);
        let (has_compaction, has_working_set) =
            resolved_compiler_includes_for_queried_sources(&queried, false, false, false, true);
        assert!(!has_compaction);
        assert!(has_working_set);
    }
}
