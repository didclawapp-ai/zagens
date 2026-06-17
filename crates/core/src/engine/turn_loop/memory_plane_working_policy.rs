//! Working-layer tool signal replay and WorkingSet path substrate (Phase 3b batch 4 / 8d).

use std::collections::HashMap;

use crate::engine::kernel_event::{KernelEvent, ToolOutcome};
use crate::engine::turn_machine::TurnKernelProjection;
use crate::working_set::path_candidates_from_tool_input;

/// Per-step scratchpad counters mirrored from the event log (reset at `ModelRequestIssued`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkingLayerStepSignals {
    pub readonly_tool_successes: u32,
    pub scratchpad_writes_this_step: u32,
}

/// Turn-level working-layer replay stats for cross-checking projection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkingLayerReplayStats {
    pub final_step: WorkingLayerStepSignals,
    pub working_set_path_touches: u32,
}

fn path_touch_from_planned(tool_name: &str, input_json: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(input_json) else {
        return false;
    };
    path_candidates_from_tool_input(tool_name, &value) > 0
}

/// Increment turn cumulative path-touch counter when a successful finish had path substrate.
pub fn record_working_set_path_touch(
    projection: &mut TurnKernelProjection,
    planned: &HashMap<String, (String, String)>,
    call_id: &str,
    outcome: &ToolOutcome,
) {
    if !matches!(outcome, ToolOutcome::Success) {
        return;
    }
    let Some((tool_name, input_json)) = planned.get(call_id) else {
        return;
    };
    if path_touch_from_planned(tool_name, input_json) {
        projection.working_set_path_touch_count += 1;
    }
}

/// Replay working-layer tool signals from kernel events (pure, no IO).
#[must_use]
pub fn simulate_working_layer_signals(events: &[KernelEvent]) -> WorkingLayerReplayStats {
    let mut step = WorkingLayerStepSignals::default();
    let mut path_touches = 0u32;
    let mut planned: HashMap<String, (String, String)> = HashMap::new();

    for event in events {
        match event {
            KernelEvent::ModelRequestIssued { .. } => {
                step = WorkingLayerStepSignals::default();
            }
            KernelEvent::ToolCallPlanned {
                call_id,
                tool_name,
                input_json,
                ..
            } => {
                planned.insert(call_id.clone(), (tool_name.clone(), input_json.clone()));
            }
            KernelEvent::ToolCallFinished {
                call_id,
                tool_name,
                outcome,
                wrote_state,
                ..
            } => {
                if matches!(outcome, ToolOutcome::Success) {
                    if let Some((planned_tool, input_json)) = planned.get(call_id)
                        && path_touch_from_planned(planned_tool, input_json)
                    {
                        path_touches += 1;
                    }
                    if *wrote_state && tool_name.starts_with("scratchpad_") {
                        step.scratchpad_writes_this_step += 1;
                    } else if !*wrote_state {
                        step.readonly_tool_successes += 1;
                    }
                }
                planned.remove(call_id);
            }
            _ => {}
        }
    }

    WorkingLayerReplayStats {
        final_step: step,
        working_set_path_touches: path_touches,
    }
}

/// Verify projection step counters and WorkingSet path touches match log replay.
#[must_use]
pub fn verify_working_layer_tool_coherence(events: &[KernelEvent]) -> Option<String> {
    let sim = simulate_working_layer_signals(events);
    if sim.working_set_path_touches == 0 && sim.final_step == WorkingLayerStepSignals::default() {
        return None;
    }
    let projection = TurnKernelProjection::from_events(events);
    let mut diffs = Vec::new();
    if sim.final_step.readonly_tool_successes != projection.readonly_tool_successes {
        diffs.push(format!(
            "readonly_tool_successes sim={} proj={}",
            sim.final_step.readonly_tool_successes, projection.readonly_tool_successes
        ));
    }
    if sim.final_step.scratchpad_writes_this_step != projection.scratchpad_writes_this_step {
        diffs.push(format!(
            "scratchpad_writes sim={} proj={}",
            sim.final_step.scratchpad_writes_this_step, projection.scratchpad_writes_this_step
        ));
    }
    if sim.working_set_path_touches != projection.working_set_path_touch_count {
        diffs.push(format!(
            "working_set_path_touches sim={} proj={}",
            sim.working_set_path_touches, projection.working_set_path_touch_count
        ));
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
    use crate::engine::kernel_event::TurnOutcome;
    use crate::engine::request_fingerprint::RequestFingerprint;
    use crate::turn::TurnLoopMode;

    fn make_fp() -> RequestFingerprint {
        RequestFingerprint {
            static_prefix_sha256: "aa".into(),
            full_prefix_sha256: "bb".into(),
        }
    }

    #[test]
    fn pure_read_fixture_working_layer_coherence() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/pure_read.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let sim = simulate_working_layer_signals(&events);
        assert_eq!(sim.final_step.readonly_tool_successes, 1);
        assert_eq!(sim.working_set_path_touches, 1);
        assert!(verify_working_layer_tool_coherence(&events).is_none());
    }

    #[test]
    fn write_batch_fixture_counts_path_touch_on_edit() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/write_batch.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let sim = simulate_working_layer_signals(&events);
        assert_eq!(sim.final_step.readonly_tool_successes, 1);
        assert_eq!(sim.working_set_path_touches, 1);
        assert!(verify_working_layer_tool_coherence(&events).is_none());
    }

    #[test]
    fn step_counters_reset_on_second_model_request() {
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
                request_fp: make_fp(),
                token_budget: 4096,
            },
            KernelEvent::ToolCallPlanned {
                turn_id: "t1".into(),
                step_idx: 1,
                call_id: "c1".into(),
                tool_name: "read_file".into(),
                input_json: r#"{"path":"a.rs"}"#.into(),
                decision: crate::engine::kernel_event::PolicyDecision::new(false, true, true),
            },
            KernelEvent::ToolCallFinished {
                turn_id: "t1".into(),
                call_id: "c1".into(),
                tool_name: "read_file".into(),
                outcome: ToolOutcome::Success,
                duration_ms: 1,
                wrote_state: false,
                result_preview: String::new(),
                session_content: String::new(),
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 2,
                request_fp: make_fp(),
                token_budget: 4096,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 2,
            },
        ];
        let sim = simulate_working_layer_signals(&events);
        assert_eq!(sim.final_step.readonly_tool_successes, 0);
        assert_eq!(sim.working_set_path_touches, 1);
        let p = TurnKernelProjection::from_events(&events);
        assert_eq!(p.working_set_path_touch_count, 1);
        assert!(verify_working_layer_tool_coherence(&events).is_none());
    }
}
