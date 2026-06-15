//! Capacity checkpoint replay field coherence (Phase 3b batch 3 / 7c).

use std::collections::{BTreeMap, HashMap};

use crate::engine::kernel_event::{CapacityAction, CapacityCheckpointKind, KernelEvent};

/// Count checkpoints with `cooldown_blocked == true`.
#[must_use]
pub fn count_capacity_cooldown_blocked(events: &[KernelEvent]) -> u32 {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                KernelEvent::CapacityCheckpoint {
                    cooldown_blocked: true,
                    ..
                }
            )
        })
        .count() as u32
}

/// Histogram of checkpoint kinds per step (observability).
#[must_use]
pub fn capacity_checkpoint_kinds_by_step(
    events: &[KernelEvent],
) -> BTreeMap<u32, HashMap<&'static str, u32>> {
    let mut out: BTreeMap<u32, HashMap<&'static str, u32>> = BTreeMap::new();
    for event in events {
        let KernelEvent::CapacityCheckpoint { step_idx, kind, .. } = event else {
            continue;
        };
        let label = match kind {
            CapacityCheckpointKind::PreRequest => "pre_request",
            CapacityCheckpointKind::PostTool => "post_tool",
            CapacityCheckpointKind::ErrorEscalation => "error_escalation",
        };
        *out.entry(*step_idx).or_default().entry(label).or_insert(0) += 1;
    }
    out
}

/// Verify capacity checkpoint fields are internally consistent with controller semantics.
///
/// - `cooldown_blocked` only appears with `action == Continue` (suppressed guardrail).
/// - `Trim` / `Handoff` never carry `cooldown_blocked`.
/// - `token_budget` must be non-zero when a checkpoint is logged.
#[must_use]
pub fn verify_capacity_checkpoint_field_coherence(events: &[KernelEvent]) -> Option<String> {
    let mut diffs = Vec::new();
    for (idx, event) in events.iter().enumerate() {
        let KernelEvent::CapacityCheckpoint {
            action,
            cooldown_blocked,
            tokens_used,
            token_budget,
            ..
        } = event
        else {
            continue;
        };

        if *token_budget == 0 {
            diffs.push(format!("checkpoint[{idx}] token_budget is zero"));
        }
        if *cooldown_blocked {
            if !matches!(action, CapacityAction::Continue) {
                diffs.push(format!(
                    "checkpoint[{idx}] cooldown_blocked with action={action:?}"
                ));
            }
        }
        if matches!(action, CapacityAction::Trim | CapacityAction::Handoff) && *cooldown_blocked {
            diffs.push(format!(
                "checkpoint[{idx}] intervention action={action:?} must not be cooldown_blocked"
            ));
        }
        if *tokens_used > token_budget.saturating_mul(4) {
            diffs.push(format!(
                "checkpoint[{idx}] tokens_used={tokens_used} implausibly exceeds token_budget={token_budget}"
            ));
        }
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
    use crate::turn::TurnLoopMode;

    #[test]
    fn capacity_checkpoint_fixture_field_coherence_passes() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/capacity_checkpoint.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        assert!(verify_capacity_checkpoint_field_coherence(&events).is_none());
        assert_eq!(count_capacity_cooldown_blocked(&events), 0);
        let kinds = capacity_checkpoint_kinds_by_step(&events);
        assert_eq!(kinds.get(&1).map(HashMap::len), Some(2));
    }

    #[test]
    fn rejects_cooldown_blocked_with_trim() {
        let events = vec![KernelEvent::CapacityCheckpoint {
            turn_id: "t1".into(),
            step_idx: 1,
            kind: CapacityCheckpointKind::PostTool,
            tokens_used: 100,
            token_budget: 200,
            action: CapacityAction::Trim,
            cooldown_blocked: true,
        }];
        assert!(verify_capacity_checkpoint_field_coherence(&events).is_some());
    }

    #[test]
    fn accepts_cooldown_blocked_continue() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 5,
            },
            KernelEvent::CapacityCheckpoint {
                turn_id: "t1".into(),
                step_idx: 1,
                kind: CapacityCheckpointKind::PreRequest,
                tokens_used: 100,
                token_budget: 200,
                action: CapacityAction::Continue,
                cooldown_blocked: true,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        assert!(verify_capacity_checkpoint_field_coherence(&events).is_none());
        assert_eq!(count_capacity_cooldown_blocked(&events), 1);
    }
}
