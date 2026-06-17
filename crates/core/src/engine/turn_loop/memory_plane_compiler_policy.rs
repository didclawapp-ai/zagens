//! ContextCompiler source mapping for memory-plane queries (Phase 3b batch 4 / 8e–8h).

use std::collections::BTreeSet;

use crate::engine::context_compiler::{BudgetOverride, BudgetPolicy, SourceId};
use crate::engine::kernel_event::KernelEvent;

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
            source_id: SourceId("working_set"),
            new_budget: BudgetPolicy::Elastic {
                min: 800,
                max: 1500,
            },
        });
    }
    if queried.contains("memory.compaction") {
        out.push(BudgetOverride {
            source_id: SourceId("memory.compaction"),
            new_budget: BudgetPolicy::Elastic {
                min: 512,
                max: 4000,
            },
        });
    }
    out
}

/// Derive ContextCompiler source ids queried this step from log projection (batch 8h / Phase D).
#[must_use]
pub fn compiler_queried_sources_from_projection(
    projection: &crate::engine::turn_machine::TurnKernelProjection,
) -> BTreeSet<String> {
    projection
        .memory_plane_queried_keys_this_step
        .iter()
        .map(|query_key| compiler_source_for_query_key(query_key).to_string())
        .filter(|source| source != "memory.unknown")
        .collect()
}

/// Verify projection-derived compiler sources match logged queries before each model request.
#[must_use]
pub fn verify_compiler_queried_sources_coherence(events: &[KernelEvent]) -> Option<String> {
    let mut step_indices = BTreeSet::new();
    for event in events {
        if let KernelEvent::MemoryPlaneQueried { step_idx, .. } = event {
            step_indices.insert(*step_idx);
        }
    }
    let mut issues = Vec::new();
    for step_idx in step_indices {
        let logged = compiler_sources_logged_at_step(events, step_idx);
        let derived = compiler_sources_from_projection_at_step(events, step_idx);
        if logged != derived {
            issues.push(format!(
                "step {step_idx} compiler_sources log={logged:?} projection={derived:?}"
            ));
        }
    }
    if issues.is_empty() {
        None
    } else {
        Some(issues.join("; "))
    }
}

/// Logged compiler source ids for a step.
#[must_use]
pub fn compiler_sources_logged_at_step(events: &[KernelEvent], step_idx: u32) -> BTreeSet<String> {
    events
        .iter()
        .filter_map(|event| {
            let KernelEvent::MemoryPlaneQueried {
                step_idx: s,
                query_key,
                ..
            } = event
            else {
                return None;
            };
            if *s != step_idx {
                return None;
            }
            Some(compiler_source_for_query_key(query_key).to_string())
        })
        .collect()
}

/// Projection-derived compiler sources immediately before a step's `ModelRequestIssued`.
#[must_use]
pub fn compiler_sources_from_projection_at_step(
    events: &[KernelEvent],
    step_idx: u32,
) -> BTreeSet<String> {
    let mut projection = crate::engine::turn_machine::TurnKernelProjection::default();
    for event in events {
        if let KernelEvent::ModelRequestIssued { step_idx: s, .. } = event
            && *s == step_idx
        {
            break;
        }
        projection.apply(event);
    }
    compiler_queried_sources_from_projection(&projection)
}

/// Verify logged `MemoryPlaneQueried.compiler_source` matches the query-key mapping table.
#[must_use]
pub fn verify_memory_plane_compiler_source_coherence(events: &[KernelEvent]) -> Option<String> {
    for event in events {
        let KernelEvent::MemoryPlaneQueried {
            step_idx,
            query_key,
            compiler_source,
            ..
        } = event
        else {
            continue;
        };
        let expected = compiler_source_for_query_key(query_key);
        if compiler_source.as_str() != expected {
            return Some(format!(
                "step {step_idx} query_key={query_key} compiler_source={compiler_source} expected={expected}"
            ));
        }
    }
    None
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
        let projection = crate::engine::turn_machine::TurnKernelProjection {
            working_set_path_touch_count: 2,
            ..Default::default()
        };
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

    #[test]
    fn compiler_source_coherence_on_memory_plane_query_fixture() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/memory_plane_query.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        assert!(
            verify_memory_plane_compiler_source_coherence(&events).is_none(),
            "memory_plane_query.json compiler source mapping failed"
        );
        assert!(
            verify_compiler_queried_sources_coherence(&events).is_none(),
            "memory_plane_query.json compiler projection coherence failed"
        );
    }

    #[test]
    fn compiler_source_coherence_detects_mismatch() {
        let events = vec![KernelEvent::MemoryPlaneQueried {
            turn_id: "t1".into(),
            step_idx: 1,
            layer: "working".into(),
            query_key: QUERY_WORKING_SET.into(),
            compiler_source: "memory.compaction".into(),
        }];
        assert!(verify_memory_plane_compiler_source_coherence(&events).is_some());
    }
}
