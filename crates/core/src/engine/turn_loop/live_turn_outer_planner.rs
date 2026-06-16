//! Phase 3b batch 5b — live outer-step effect planner skeleton.
//!
//! Describes the canonical v3 pre-inner-step effect chain. Live IO still runs
//! through `TurnLoopHost` + `EffectInterpreter`; this module is the pure plan
//! surface that will eventually drive `TurnMachine::step` + interpreter.

use crate::capacity::GuardrailAction;
use crate::engine::kernel_event::CapacityCheckpointKind;
use crate::engine::turn_loop::continuation_boundary_policy::OuterBoundaryKind;
use crate::engine::turn_machine::{Effect, capacity_cooldown_backoff_millis};

/// Baseline v3 effects attempted before each inner step (model request).
#[derive(Debug, Clone)]
pub struct PreInnerStepEffectPlan {
    pub baseline: Vec<Effect>,
}

/// Capacity checkpoint tail aligned with [`ReplayTurnMachine`] on `CapacityCheckpoint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityCheckpointEffectTail {
    None,
    RunCompaction,
    Sleep { millis: u64 },
}

/// Planned baseline before auto-compaction + layered-context checkpoint IO.
#[must_use]
pub fn plan_v3_pre_inner_step_baseline() -> PreInnerStepEffectPlan {
    PreInnerStepEffectPlan {
        baseline: vec![Effect::RunCompaction, Effect::RunLayeredContextCheckpoint],
    }
}

/// Label for planner baseline slot logging (0 = auto-compaction, 1 = layered context).
#[must_use]
pub fn pre_inner_step_baseline_effect_label(slot: usize) -> Option<&'static str> {
    match slot {
        0 => Some("RunCompaction"),
        1 => Some("RunLayeredContextCheckpoint"),
        _ => None,
    }
}

/// Replay-aligned effect for an outer-loop boundary grant (TurnMachine substrate).
#[must_use]
pub fn plan_outer_boundary_replay_effect(kind: OuterBoundaryKind) -> Option<Effect> {
    match kind {
        OuterBoundaryKind::StepLimit | OuterBoundaryKind::LoopGuard => Some(Effect::InjectSteer {
            text: String::new(),
        }),
        OuterBoundaryKind::ContextOverflowCycleHandoff | OuterBoundaryKind::InTurnCycleAdvance => {
            Some(Effect::InjectSteer {
                text: String::new(),
            })
        }
        OuterBoundaryKind::PreRequestCapacityHold
        | OuterBoundaryKind::ErrorEscalationCapacityHold => None,
    }
}

/// Compare planned vs interpreted outer-boundary replay effects (v3 shadow bake).
#[must_use]
pub fn verify_outer_boundary_effect_alignment(
    kind: OuterBoundaryKind,
    interpreted: Option<Effect>,
) -> Option<String> {
    let planned = plan_outer_boundary_replay_effect(kind);
    match (planned, interpreted) {
        (None, None) => None,
        (Some(p), Some(i)) if effect_kind_matches(&p, &i) => None,
        (planned, interpreted) => Some(format!(
            "outer boundary {kind:?} planned={planned:?} interpreted={interpreted:?}"
        )),
    }
}

fn effect_kind_matches(planned: &Effect, interpreted: &Effect) -> bool {
    matches!(
        (planned, interpreted),
        (Effect::InjectSteer { .. }, Effect::InjectSteer { .. })
            | (Effect::RunCompaction, Effect::RunCompaction)
            | (Effect::Sleep { .. }, Effect::Sleep { .. })
    )
}

/// Derive replay-aligned capacity tail from checkpoint metadata.
#[must_use]
pub fn capacity_checkpoint_effect_tail(
    trim_or_handoff: bool,
    cooldown_blocked: bool,
) -> CapacityCheckpointEffectTail {
    if cooldown_blocked {
        return CapacityCheckpointEffectTail::Sleep {
            millis: capacity_cooldown_backoff_millis(),
        };
    }
    if trim_or_handoff {
        return CapacityCheckpointEffectTail::RunCompaction;
    }
    CapacityCheckpointEffectTail::None
}

/// Whether a guardrail action maps to trim/handoff `RunCompaction` replay tail.
#[must_use]
pub fn capacity_trim_or_handoff_from_action(action: GuardrailAction) -> bool {
    matches!(
        action,
        GuardrailAction::TargetedContextRefresh | GuardrailAction::VerifyAndReplan
    )
}

/// Planned capacity checkpoint tail from live decision metadata.
#[must_use]
pub fn plan_capacity_checkpoint_effect_tail(
    action: GuardrailAction,
    cooldown_blocked: bool,
) -> CapacityCheckpointEffectTail {
    capacity_checkpoint_effect_tail(
        capacity_trim_or_handoff_from_action(action),
        cooldown_blocked,
    )
}

/// Map a capacity checkpoint site to the outer-loop hold boundary (when hold applies).
#[must_use]
pub fn capacity_hold_boundary_for_checkpoint(
    kind: CapacityCheckpointKind,
) -> Option<OuterBoundaryKind> {
    match kind {
        CapacityCheckpointKind::PreRequest => Some(OuterBoundaryKind::PreRequestCapacityHold),
        CapacityCheckpointKind::ErrorEscalation => {
            Some(OuterBoundaryKind::ErrorEscalationCapacityHold)
        }
        CapacityCheckpointKind::PostTool => None,
    }
}

/// Planned effect tail when a capacity hold boundary fires after checkpoint IO.
#[must_use]
pub fn plan_capacity_hold_boundary_effect(
    action: GuardrailAction,
    cooldown_blocked: bool,
) -> CapacityCheckpointEffectTail {
    plan_capacity_checkpoint_effect_tail(action, cooldown_blocked)
}

/// Human-readable effect label for capacity hold planner logging.
#[must_use]
pub fn capacity_checkpoint_effect_tail_label(tail: CapacityCheckpointEffectTail) -> &'static str {
    match tail {
        CapacityCheckpointEffectTail::None => "None",
        CapacityCheckpointEffectTail::RunCompaction => "RunCompaction",
        CapacityCheckpointEffectTail::Sleep { .. } => "Sleep",
    }
}

/// Compare planned vs interpreted capacity tails (v3 shadow bake).
#[must_use]
pub fn verify_capacity_tail_alignment(
    planned: CapacityCheckpointEffectTail,
    interpreted: CapacityCheckpointEffectTail,
) -> Option<String> {
    if planned == interpreted {
        return None;
    }
    Some(format!(
        "capacity tail planned={planned:?} interpreted={interpreted:?}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_inner_step_baseline_matches_v3_run_order() {
        let plan = plan_v3_pre_inner_step_baseline();
        assert_eq!(plan.baseline.len(), 2);
        assert!(matches!(plan.baseline[0], Effect::RunCompaction));
        assert!(matches!(
            plan.baseline[1],
            Effect::RunLayeredContextCheckpoint
        ));
    }

    #[test]
    fn capacity_tail_prefers_sleep_when_cooldown_blocked() {
        assert_eq!(
            capacity_checkpoint_effect_tail(true, true),
            CapacityCheckpointEffectTail::Sleep {
                millis: capacity_cooldown_backoff_millis()
            }
        );
    }

    #[test]
    fn capacity_tail_run_compaction_on_trim_without_cooldown() {
        assert_eq!(
            capacity_checkpoint_effect_tail(true, false),
            CapacityCheckpointEffectTail::RunCompaction
        );
    }

    #[test]
    fn capacity_tail_none_when_no_intervention() {
        assert_eq!(
            capacity_checkpoint_effect_tail(false, false),
            CapacityCheckpointEffectTail::None
        );
    }

    #[test]
    fn plan_capacity_tail_sleep_when_cooldown_blocked() {
        assert_eq!(
            plan_capacity_checkpoint_effect_tail(GuardrailAction::TargetedContextRefresh, true),
            CapacityCheckpointEffectTail::Sleep {
                millis: capacity_cooldown_backoff_millis()
            }
        );
    }

    #[test]
    fn plan_capacity_tail_compaction_on_handoff() {
        assert_eq!(
            plan_capacity_checkpoint_effect_tail(GuardrailAction::VerifyAndReplan, false),
            CapacityCheckpointEffectTail::RunCompaction
        );
    }

    #[test]
    fn verify_capacity_tail_reports_mismatch() {
        assert!(
            verify_capacity_tail_alignment(
                CapacityCheckpointEffectTail::RunCompaction,
                CapacityCheckpointEffectTail::None
            )
            .is_some()
        );
    }

    #[test]
    fn outer_boundary_step_limit_maps_to_inject_steer() {
        assert!(matches!(
            plan_outer_boundary_replay_effect(OuterBoundaryKind::StepLimit),
            Some(Effect::InjectSteer { .. })
        ));
    }

    #[test]
    fn outer_boundary_capacity_hold_has_no_replay_effect() {
        assert!(
            plan_outer_boundary_replay_effect(OuterBoundaryKind::PreRequestCapacityHold).is_none()
        );
    }

    #[test]
    fn capacity_hold_boundary_maps_checkpoint_kind() {
        assert_eq!(
            capacity_hold_boundary_for_checkpoint(CapacityCheckpointKind::PreRequest),
            Some(OuterBoundaryKind::PreRequestCapacityHold)
        );
        assert_eq!(
            capacity_hold_boundary_for_checkpoint(CapacityCheckpointKind::ErrorEscalation),
            Some(OuterBoundaryKind::ErrorEscalationCapacityHold)
        );
        assert_eq!(
            capacity_hold_boundary_for_checkpoint(CapacityCheckpointKind::PostTool),
            None
        );
    }

    #[test]
    fn capacity_hold_planner_delegates_to_checkpoint_tail() {
        assert_eq!(
            plan_capacity_hold_boundary_effect(GuardrailAction::VerifyAndReplan, false),
            CapacityCheckpointEffectTail::RunCompaction
        );
    }
}
