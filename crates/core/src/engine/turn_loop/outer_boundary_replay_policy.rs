//! Outer-loop boundary replay coherence (Phase 3b batch 5b cont.).
//!
//! Validates that continuation / overflow / cycle grants on a turn log respect
//! the same caps encoded in [`continuation_boundary_policy`].

use crate::engine::kernel_event::{KernelEvent, OverflowStrategy};
use crate::engine::turn_loop::continuation_boundary_policy::{
    max_context_cycle_handoffs, max_in_turn_cycle_advances, max_loop_guard_grants,
    max_step_limit_grants,
};
use crate::turn::TurnLoopMode;

fn count_step_limit_continuations(events: &[KernelEvent]) -> u32 {
    events
        .iter()
        .filter(|event| matches!(event, KernelEvent::StepLimitContinuation { .. }))
        .count() as u32
}

fn count_loop_guard_continuations(events: &[KernelEvent]) -> u32 {
    events
        .iter()
        .filter(|event| matches!(event, KernelEvent::LoopGuardContinuation { .. }))
        .count() as u32
}

fn count_cycle_handoffs(events: &[KernelEvent]) -> u32 {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                KernelEvent::ContextOverflowRecovered {
                    strategy: OverflowStrategy::CycleHandoff,
                    ..
                }
            )
        })
        .count() as u32
}

fn count_in_turn_cycle_advances(events: &[KernelEvent]) -> u32 {
    events
        .iter()
        .filter(|event| matches!(event, KernelEvent::CycleAdvanced { .. }))
        .count() as u32
}

/// Verify outer-boundary grant counts on a turn log stay within policy caps.
#[must_use]
pub fn verify_outer_boundary_event_caps(
    events: &[KernelEvent],
    mode: TurnLoopMode,
) -> Option<String> {
    if mode.is_plan() {
        let step_limit = count_step_limit_continuations(events);
        let loop_guard = count_loop_guard_continuations(events);
        let cycle_handoffs = count_cycle_handoffs(events);
        let in_turn_advances = count_in_turn_cycle_advances(events);
        if step_limit > 0 || loop_guard > 0 || cycle_handoffs > 0 || in_turn_advances > 0 {
            return Some(format!(
                "plan mode turn emitted outer-boundary grants \
                 (step_limit={step_limit}, loop_guard={loop_guard}, \
                 cycle_handoffs={cycle_handoffs}, in_turn_advances={in_turn_advances})"
            ));
        }
        return None;
    }

    let mut diffs = Vec::new();
    let step_limit = count_step_limit_continuations(events);
    if step_limit > max_step_limit_grants() {
        diffs.push(format!(
            "step_limit_continuations events={step_limit} cap={}",
            max_step_limit_grants()
        ));
    }
    let loop_guard = count_loop_guard_continuations(events);
    if loop_guard > max_loop_guard_grants() {
        diffs.push(format!(
            "loop_guard_continuations events={loop_guard} cap={}",
            max_loop_guard_grants()
        ));
    }
    let cycle_handoffs = count_cycle_handoffs(events);
    if cycle_handoffs > max_context_cycle_handoffs() {
        diffs.push(format!(
            "cycle_handoffs events={cycle_handoffs} cap={}",
            max_context_cycle_handoffs()
        ));
    }
    let in_turn_advances = count_in_turn_cycle_advances(events);
    if in_turn_advances > max_in_turn_cycle_advances() {
        diffs.push(format!(
            "in_turn_cycle_advances events={in_turn_advances} cap={}",
            max_in_turn_cycle_advances()
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

    #[test]
    fn plan_mode_rejects_any_outer_boundary_grant() {
        let events = vec![KernelEvent::StepLimitContinuation {
            turn_id: "t1".into(),
            step_idx: 1,
            lht_objective_injected: true,
        }];
        assert!(verify_outer_boundary_event_caps(&events, TurnLoopMode::Plan).is_some());
    }

    #[test]
    fn agent_mode_allows_fixture_shaped_grants() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/lht_continue.json");
        let raw = std::fs::read_to_string(path).expect("fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        assert!(verify_outer_boundary_event_caps(&events, TurnLoopMode::Agent).is_none());
    }

    #[test]
    fn agent_mode_flags_grant_above_cap() {
        let mut events = vec![KernelEvent::TurnStarted {
            turn_id: "t1".into(),
            mode: TurnLoopMode::Agent,
            input_text: "x".into(),
            max_steps: 20,
        }];
        for step_idx in 0..=max_step_limit_grants() {
            events.push(KernelEvent::StepLimitContinuation {
                turn_id: "t1".into(),
                step_idx,
                lht_objective_injected: true,
            });
        }
        events.push(KernelEvent::TurnEnded {
            turn_id: "t1".into(),
            outcome: TurnOutcome::Completed,
            total_steps: 1,
        });
        assert!(verify_outer_boundary_event_caps(&events, TurnLoopMode::Agent).is_some());
    }
}
