//! Kernel Trace Report compare document (KTR P2).

use anyhow::Context;
use serde::{Deserialize, Serialize};

use super::trace_bundle::{TRACE_BUNDLE_SCHEMA_VERSION, TraceBundle, TraceEventEnvelope};

pub const TRACE_COMPARE_DOCUMENT_KIND: &str = "compare";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceCompareSide {
    pub label: String,
    pub bundle: TraceBundle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceEffectCountDelta {
    pub field: String,
    pub left: u32,
    pub right: u32,
    pub delta: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceCompareDiff {
    pub coherence_match: bool,
    pub left_coherence_ok: bool,
    pub right_coherence_ok: bool,
    pub event_kind_sequence_match: bool,
    pub left_event_kinds: Vec<String>,
    pub right_event_kinds: Vec<String>,
    pub first_kind_mismatch_index: Option<usize>,
    pub left_event_count: usize,
    pub right_event_count: usize,
    pub turn_count_left: usize,
    pub turn_count_right: usize,
    pub effect_count_deltas: Vec<TraceEffectCountDelta>,
    pub guard_event_deltas: Vec<TraceGuardEventDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceGuardEventDelta {
    pub kind: String,
    pub left: u32,
    pub right: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceCompareDocument {
    pub document_kind: String,
    pub schema_version: u32,
    pub generator: super::trace_bundle::TraceBundleGenerator,
    pub left: TraceCompareSide,
    pub right: TraceCompareSide,
    pub diff: TraceCompareDiff,
}

fn event_kinds(events: &[TraceEventEnvelope]) -> Vec<String> {
    events
        .iter()
        .filter_map(|entry| {
            entry
                .payload
                .get("event_type")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect()
}

fn count_kinds(kinds: &[String], targets: &[&str]) -> u32 {
    kinds
        .iter()
        .filter(|k| targets.contains(&k.as_str()))
        .count() as u32
}

const GUARD_KINDS: &[&str] = &[
    "steer_injected",
    "step_limit_continuation",
    "loop_guard_continuation",
    "loop_guard_triggered",
    "capacity_checkpoint",
    "cycle_advanced",
    "deferred_tool_activated",
];

fn effect_count_deltas(
    left: &super::turn_machine::ReplayEffectCounts,
    right: &super::turn_machine::ReplayEffectCounts,
) -> Vec<TraceEffectCountDelta> {
    let pairs = [
        ("call_model", left.call_model, right.call_model),
        ("execute_batch", left.execute_batch, right.execute_batch),
        (
            "request_approval",
            left.request_approval,
            right.request_approval,
        ),
        ("inject_steer", left.inject_steer, right.inject_steer),
        ("run_compaction", left.run_compaction, right.run_compaction),
        ("notify_lsp", left.notify_lsp, right.notify_lsp),
        ("sleep", left.sleep, right.sleep),
        ("query_memory", left.query_memory, right.query_memory),
        (
            "run_layered_context_checkpoint",
            left.run_layered_context_checkpoint,
            right.run_layered_context_checkpoint,
        ),
        (
            "refresh_system_prompt",
            left.refresh_system_prompt,
            right.refresh_system_prompt,
        ),
        ("emit_artifact", left.emit_artifact, right.emit_artifact),
    ];
    pairs
        .into_iter()
        .filter_map(|(field, l, r)| {
            if l == r {
                None
            } else {
                let delta = i64::from(r) - i64::from(l);
                Some(TraceEffectCountDelta {
                    field: field.to_string(),
                    left: l,
                    right: r,
                    delta: i32::try_from(delta).unwrap_or(if delta > 0 {
                        i32::MAX
                    } else {
                        i32::MIN
                    }),
                })
            }
        })
        .collect()
}

fn guard_event_deltas(left_kinds: &[String], right_kinds: &[String]) -> Vec<TraceGuardEventDelta> {
    GUARD_KINDS
        .iter()
        .filter_map(|kind| {
            let l = count_kinds(left_kinds, &[*kind]);
            let r = count_kinds(right_kinds, &[*kind]);
            if l == r {
                None
            } else {
                Some(TraceGuardEventDelta {
                    kind: (*kind).to_string(),
                    left: l,
                    right: r,
                })
            }
        })
        .collect()
}

fn first_kind_mismatch(left: &[String], right: &[String]) -> Option<usize> {
    let max = left.len().max(right.len());
    for idx in 0..max {
        let l = left.get(idx).map(String::as_str);
        let r = right.get(idx).map(String::as_str);
        if l != r {
            return Some(idx);
        }
    }
    None
}

/// Build a compare document from two trace bundles (thread or fixture).
#[must_use]
pub fn build_trace_compare_document(
    left_label: String,
    left: TraceBundle,
    right_label: String,
    right: TraceBundle,
) -> TraceCompareDocument {
    let left_kinds = event_kinds(&left.events);
    let right_kinds = event_kinds(&right.events);
    let left_coherence_ok = left.replay_summary.coherence_ok;
    let right_coherence_ok = right.replay_summary.coherence_ok;

    TraceCompareDocument {
        document_kind: TRACE_COMPARE_DOCUMENT_KIND.to_string(),
        schema_version: TRACE_BUNDLE_SCHEMA_VERSION,
        generator: left.generator.clone(),
        left: TraceCompareSide {
            label: left_label,
            bundle: left.clone(),
        },
        right: TraceCompareSide {
            label: right_label,
            bundle: right.clone(),
        },
        diff: TraceCompareDiff {
            coherence_match: left_coherence_ok == right_coherence_ok,
            left_coherence_ok,
            right_coherence_ok,
            event_kind_sequence_match: left_kinds == right_kinds,
            left_event_kinds: left_kinds.clone(),
            right_event_kinds: right_kinds.clone(),
            first_kind_mismatch_index: first_kind_mismatch(&left_kinds, &right_kinds),
            left_event_count: left.events.len(),
            right_event_count: right.events.len(),
            turn_count_left: left.replay_summary.turns.len(),
            turn_count_right: right.replay_summary.turns.len(),
            effect_count_deltas: effect_count_deltas(
                &left.replay_summary.effect_counts,
                &right.replay_summary.effect_counts,
            ),
            guard_event_deltas: guard_event_deltas(&left_kinds, &right_kinds),
        },
    }
}

/// Serialize compare document to pretty JSON.
pub fn trace_compare_to_json(doc: &TraceCompareDocument) -> anyhow::Result<String> {
    serde_json::to_string_pretty(doc).context("serialize trace compare document")
}

/// Embed compare JSON into the HTML shell (same JSON slot as single report).
pub fn embed_trace_compare_in_html(
    template: &str,
    doc: &TraceCompareDocument,
) -> anyhow::Result<String> {
    use super::trace_bundle::embed_json_in_trace_shell;
    let json = trace_compare_to_json(doc)?;
    embed_json_in_trace_shell(template, &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay")
            .join(name)
    }

    #[test]
    fn lht_continue_vs_loop_guard_differs_in_event_sequence() {
        use super::super::trace_bundle::build_trace_bundle_from_fixture;

        let left = build_trace_bundle_from_fixture(&fixture_path("lht_continue.json")).unwrap();
        let right = build_trace_bundle_from_fixture(&fixture_path("loop_guard.json")).unwrap();
        let doc = build_trace_compare_document(
            "lht_continue".to_string(),
            left,
            "loop_guard".to_string(),
            right,
        );
        assert_eq!(doc.document_kind, TRACE_COMPARE_DOCUMENT_KIND);
        assert!(!doc.diff.event_kind_sequence_match);
        assert!(doc.diff.first_kind_mismatch_index.is_some());
    }

    #[test]
    fn identical_fixture_compare_is_clean() {
        use super::super::trace_bundle::build_trace_bundle_from_fixture;

        let left = build_trace_bundle_from_fixture(&fixture_path("pure_read.json")).unwrap();
        let right = build_trace_bundle_from_fixture(&fixture_path("pure_read.json")).unwrap();
        let doc = build_trace_compare_document(
            "pure_read_a".to_string(),
            left,
            "pure_read_b".to_string(),
            right,
        );
        assert!(doc.diff.event_kind_sequence_match);
        assert!(doc.diff.effect_count_deltas.is_empty());
    }
}
