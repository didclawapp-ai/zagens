//! Pure guard/capacity projection helpers for Phase 3b batch 3 (replay substrate).

use crate::engine::kernel_event::{CapacityAction, KernelEvent};

/// Count `LoopGuardTriggered` events on a turn log.
#[must_use]
pub fn count_loop_guard_triggered(events: &[KernelEvent]) -> u32 {
    events
        .iter()
        .filter(|event| matches!(event, KernelEvent::LoopGuardTriggered { .. }))
        .count() as u32
}

/// Count `CapacityCheckpoint` events on a turn log.
#[must_use]
pub fn count_capacity_checkpoints(events: &[KernelEvent]) -> u32 {
    events
        .iter()
        .filter(|event| matches!(event, KernelEvent::CapacityCheckpoint { .. }))
        .count() as u32
}

/// Last observed capacity checkpoint action (most recent wins).
#[must_use]
pub fn last_capacity_checkpoint_action(events: &[KernelEvent]) -> Option<CapacityAction> {
    events.iter().rev().find_map(|event| {
        let KernelEvent::CapacityCheckpoint { action, .. } = event else {
            return None;
        };
        Some(action.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::kernel_event::{
        CapacityAction, CapacityCheckpointKind, KernelEvent, TurnOutcome,
    };
    use crate::turn::TurnLoopMode;

    #[test]
    fn counts_loop_guard_and_capacity_from_fixture_shape() {
        let events = vec![
            KernelEvent::CapacityCheckpoint {
                turn_id: "t1".into(),
                step_idx: 1,
                kind: CapacityCheckpointKind::PreRequest,
                tokens_used: 100,
                token_budget: 200,
                action: CapacityAction::Continue,
                cooldown_blocked: false,
            },
            KernelEvent::LoopGuardTriggered {
                turn_id: "t1".into(),
                call_id: "c1".into(),
                reason: "identical_tool_call".into(),
            },
            KernelEvent::CapacityCheckpoint {
                turn_id: "t1".into(),
                step_idx: 1,
                kind: CapacityCheckpointKind::PostTool,
                tokens_used: 150,
                token_budget: 200,
                action: CapacityAction::Trim,
                cooldown_blocked: false,
            },
        ];
        assert_eq!(count_loop_guard_triggered(&events), 1);
        assert_eq!(count_capacity_checkpoints(&events), 2);
        assert_eq!(
            last_capacity_checkpoint_action(&events),
            Some(CapacityAction::Trim)
        );
    }

    #[test]
    fn loop_guard_fixture_guard_projection_passes() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/loop_guard.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        assert_eq!(count_loop_guard_triggered(&events), 1);
        let p = crate::engine::turn_machine::TurnKernelProjection::from_events(&events);
        assert_eq!(p.loop_guard_triggered_count, 1);
        assert_eq!(p.loop_guard_continuations, 1);
        assert!(p.outcome.is_some());
        assert_eq!(p.outcome, Some(TurnOutcome::Completed));
        assert!(matches!(p.mode, Some(TurnLoopMode::Agent)));
    }
}
