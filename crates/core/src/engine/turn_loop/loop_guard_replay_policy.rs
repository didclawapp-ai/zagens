//! Replay LoopGuard trigger expectations from kernel events (Phase 3b batch 3).

use std::collections::BTreeMap;

use serde_json::Value;

use crate::engine::kernel_event::{KernelEvent, ToolOutcome};
use crate::engine::loop_guard::{AttemptDecision, LoopGuard, OutcomeDecision};

use super::guard_projection_policy::count_loop_guard_triggered;

/// Simulated trigger counts from `ToolCallPlanned` / `ToolCallFinished` replay.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoopGuardTriggerExpectations {
    pub identical_blocks: u32,
    pub failure_halts: u32,
}

/// Observed `LoopGuardTriggered` counts grouped by `reason`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoopGuardTriggerLogCounts {
    pub identical_tool_call: u32,
    pub failure_halt: u32,
    pub deferred_set_area_batch: u32,
    pub other: u32,
}

/// Count trigger reasons on a turn log.
#[must_use]
pub fn count_loop_guard_triggers_by_reason(events: &[KernelEvent]) -> LoopGuardTriggerLogCounts {
    let mut counts = LoopGuardTriggerLogCounts::default();
    for event in events {
        let KernelEvent::LoopGuardTriggered { reason, .. } = event else {
            continue;
        };
        match reason.as_str() {
            "identical_tool_call" => counts.identical_tool_call += 1,
            "failure_halt" => counts.failure_halt += 1,
            "deferred_set_area_batch" => counts.deferred_set_area_batch += 1,
            _ => counts.other += 1,
        }
    }
    counts
}

/// Re-simulate [`LoopGuard`] decisions from planned/finished tool rows in the log.
#[must_use]
pub fn simulate_loop_guard_trigger_expectations(
    events: &[KernelEvent],
) -> LoopGuardTriggerExpectations {
    let mut guard = LoopGuard::default();
    let mut out = LoopGuardTriggerExpectations::default();
    for event in events {
        match event {
            KernelEvent::ToolCallPlanned {
                tool_name,
                input_json,
                ..
            } => {
                let input: Value = serde_json::from_str(input_json).unwrap_or(Value::Null);
                if matches!(
                    guard.record_attempt(tool_name, &input),
                    AttemptDecision::Block(_)
                ) {
                    out.identical_blocks += 1;
                }
            }
            KernelEvent::ToolCallFinished {
                tool_name,
                outcome,
                wrote_state,
                ..
            } => {
                let ok = matches!(outcome, ToolOutcome::Success);
                if matches!(
                    guard.record_outcome(tool_name, ok),
                    OutcomeDecision::Halt(_)
                ) {
                    out.failure_halts += 1;
                }
                if ok && *wrote_state && LoopGuard::is_state_mutating_tool(tool_name) {
                    guard.note_state_changed();
                }
            }
            KernelEvent::LoopGuardContinuation { .. } => guard.reset_failures(),
            _ => {}
        }
    }
    out
}

fn call_id_referenced_in_turn(events: &[KernelEvent], call_id: &str) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            KernelEvent::ToolCallPlanned { call_id: id, .. }
                | KernelEvent::ToolCallStarted { call_id: id, .. }
                | KernelEvent::ToolCallFinished { call_id: id, .. }
                if id == call_id
        )
    })
}

/// Verify log `LoopGuardTriggered` anchors are consistent with replay simulation.
#[must_use]
pub fn verify_loop_guard_replay_coherence(events: &[KernelEvent]) -> Option<String> {
    if count_loop_guard_triggered(events) == 0 {
        return None;
    }

    let log = count_loop_guard_triggers_by_reason(events);
    let sim = simulate_loop_guard_trigger_expectations(events);
    let mut diffs = Vec::new();

    if log.failure_halt != sim.failure_halts {
        diffs.push(format!(
            "failure_halt log={} sim={}",
            log.failure_halt, sim.failure_halts
        ));
    }
    if log.identical_tool_call < sim.identical_blocks {
        diffs.push(format!(
            "identical_tool_call log={} < sim={}",
            log.identical_tool_call, sim.identical_blocks
        ));
    }

    for event in events {
        let KernelEvent::LoopGuardTriggered { call_id, .. } = event else {
            continue;
        };
        if !call_id_referenced_in_turn(events, call_id) {
            diffs.push(format!("loop_guard_trigger call_id {call_id} unreferenced"));
        }
    }

    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("; "))
    }
}

/// Per-step trigger reason histogram (observability).
#[must_use]
pub fn loop_guard_trigger_reasons_by_step(
    events: &[KernelEvent],
) -> BTreeMap<u32, BTreeMap<String, u32>> {
    let mut current_step = 0u32;
    let mut out: BTreeMap<u32, BTreeMap<String, u32>> = BTreeMap::new();
    for event in events {
        if let KernelEvent::ModelRequestIssued { step_idx, .. } = event {
            current_step = *step_idx;
        }
        let KernelEvent::LoopGuardTriggered { reason, .. } = event else {
            continue;
        };
        *out.entry(current_step)
            .or_default()
            .entry(reason.clone())
            .or_insert(0) += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::kernel_event::{PolicyDecision, TurnOutcome};
    use crate::turn::TurnLoopMode;

    #[test]
    fn loop_guard_fixture_replay_coherence_passes() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/loop_guard.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        assert!(verify_loop_guard_replay_coherence(&events).is_none());
        let log = count_loop_guard_triggers_by_reason(&events);
        assert_eq!(log.identical_tool_call, 1);
        let sim = simulate_loop_guard_trigger_expectations(&events);
        assert_eq!(sim.identical_blocks, 0);
        assert_eq!(sim.failure_halts, 0);
    }

    #[test]
    fn failure_halt_simulation_matches_log() {
        let mut events = vec![KernelEvent::TurnStarted {
            turn_id: "t1".into(),
            mode: TurnLoopMode::Agent,
            input_text: "x".into(),
            max_steps: 10,
        }];
        for i in 0..8 {
            let call_id = format!("call-{i}");
            events.push(KernelEvent::ToolCallPlanned {
                turn_id: "t1".into(),
                step_idx: 1,
                call_id: call_id.clone(),
                tool_name: "exec_shell".into(),
                input_json: format!(r#"{{"command":"false-{i}"}}"#),
                decision: PolicyDecision::new(false, false, false),
            });
            events.push(KernelEvent::ToolCallFinished {
                turn_id: "t1".into(),
                call_id,
                tool_name: "exec_shell".into(),
                outcome: ToolOutcome::ToolError {
                    message: "failed".into(),
                },
                duration_ms: 1,
                wrote_state: false,
            });
        }
        events.push(KernelEvent::LoopGuardTriggered {
            turn_id: "t1".into(),
            call_id: "call-7".into(),
            reason: "failure_halt".into(),
        });
        events.push(KernelEvent::TurnEnded {
            turn_id: "t1".into(),
            outcome: TurnOutcome::Completed,
            total_steps: 1,
        });
        let sim = simulate_loop_guard_trigger_expectations(&events);
        assert_eq!(sim.failure_halts, 1);
        let log = count_loop_guard_triggers_by_reason(&events);
        assert_eq!(log.failure_halt, 1);
        let coherence = verify_loop_guard_replay_coherence(&events);
        assert!(
            coherence.is_none(),
            "coherence error: {:?}, sim={:?}",
            coherence,
            sim
        );
    }

    #[test]
    fn identical_block_simulation_matches_log_when_planned() {
        let events = vec![
            KernelEvent::ToolCallPlanned {
                turn_id: "t1".into(),
                step_idx: 1,
                call_id: "c1".into(),
                tool_name: "read_file".into(),
                input_json: r#"{"path":"a.rs"}"#.into(),
                decision: PolicyDecision::new(false, true, true),
            },
            KernelEvent::ToolCallPlanned {
                turn_id: "t1".into(),
                step_idx: 1,
                call_id: "c2".into(),
                tool_name: "read_file".into(),
                input_json: r#"{"path":"a.rs"}"#.into(),
                decision: PolicyDecision::new(false, true, true),
            },
            KernelEvent::ToolCallPlanned {
                turn_id: "t1".into(),
                step_idx: 1,
                call_id: "c3".into(),
                tool_name: "read_file".into(),
                input_json: r#"{"path":"a.rs"}"#.into(),
                decision: PolicyDecision::new(false, true, true),
            },
            KernelEvent::LoopGuardTriggered {
                turn_id: "t1".into(),
                call_id: "c3".into(),
                reason: "identical_tool_call".into(),
            },
        ];
        let sim = simulate_loop_guard_trigger_expectations(&events);
        assert_eq!(sim.identical_blocks, 1);
        assert!(verify_loop_guard_replay_coherence(&events).is_none());
    }
}
