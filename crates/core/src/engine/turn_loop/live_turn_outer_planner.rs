//! Phase 3b batch 5b — live outer-step effect planner skeleton.
//!
//! Describes the canonical v3 pre-inner-step effect chain. Live IO still runs
//! through `V3TurnHost` + `EffectInterpreter`; this module is the pure plan
//! surface that will eventually drive `TurnMachine::step` + interpreter.

use crate::capacity::GuardrailAction;
use crate::engine::kernel_event::CapacityCheckpointKind;
use crate::engine::turn_loop::continuation_boundary_policy::OuterBoundaryKind;
use crate::engine::turn_loop::system_prompt_refresh_policy::{
    SystemPromptRefreshPlan, plan_system_prompt_refresh,
};
use crate::engine::turn_machine::{Effect, capacity_cooldown_backoff_millis};

/// Baseline v3 effects attempted before each inner step (model request).
#[derive(Debug, Clone)]
pub struct PreInnerStepEffectPlan {
    pub baseline: Vec<Effect>,
}

/// Per-outer-loop-iteration frame reset before pre-inner work (batch 5b cont.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OuterStepFrameEffectPlan {
    /// Per-step scratchpad accumulator reset (host IO until `TurnMachine::step`).
    pub scratchpad_step_reset: bool,
    /// Sync active turn id + step for out-of-loop memory / projection events.
    pub kernel_turn_frame_sync: bool,
    /// Cooperative cancel check before pre-inner segment (host token; no replay effect).
    pub cancel_check: bool,
}

/// Canonical v3 outer step-frame seam (scratchpad reset + kernel frame sync + cancel gate).
#[must_use]
pub fn plan_v3_outer_step_frame_effects() -> OuterStepFrameEffectPlan {
    OuterStepFrameEffectPlan {
        scratchpad_step_reset: true,
        kernel_turn_frame_sync: true,
        cancel_check: true,
    }
}

/// Full v3 outer pre-inner-step effect plan (batch 5b cont. — TurnMachine substrate).
#[derive(Debug, Clone)]
pub struct OuterPreInnerEffectPlan {
    /// Each drained `rx_steer` message maps to `Effect::InjectSteer` (v3/shadow).
    pub live_steer_inject_per_message: bool,
    /// Host refresh IO + parallel `QueryMemory` plan (compiler source graph target).
    pub system_prompt_refresh: SystemPromptRefreshPlan,
    pub baseline: Vec<Effect>,
}

/// Conditional v3 outer post-inner-step effect slots (after streaming/tool IO).
#[derive(Debug, Clone)]
pub struct OuterPostInnerEffectPlan {
    /// Fired when loop-guard halts and LHT continuation is granted.
    pub loop_guard_continuation: Option<Effect>,
    /// Capacity hold boundary may fire after tool errors (no replay effect).
    pub error_escalation_capacity_hold: bool,
    /// Fired when in-turn cycle advance gate opens mid-turn.
    pub in_turn_cycle_advance: Option<Effect>,
}

/// Canonical v3 slots before inner step IO (baseline + documented host refresh seam).
#[must_use]
pub fn plan_v3_outer_pre_inner_step_effects() -> OuterPreInnerEffectPlan {
    OuterPreInnerEffectPlan {
        live_steer_inject_per_message: true,
        system_prompt_refresh: plan_system_prompt_refresh(),
        baseline: plan_v3_pre_inner_step_baseline().baseline,
    }
}

/// Replay-aligned effect for one live steer drain (`rx_steer` → `inject_live_steer`).
#[must_use]
pub fn plan_live_steer_inject_effect(text: String) -> Effect {
    Effect::InjectSteer { text }
}

/// Planned inject-steer chain for a drained steer batch (observability / replay substrate).
#[must_use]
pub fn plan_live_steer_inject_effects(texts: impl IntoIterator<Item = String>) -> Vec<Effect> {
    texts
        .into_iter()
        .map(plan_live_steer_inject_effect)
        .collect()
}

/// Template for conditional post-inner outer boundaries (eligibility still host-gated).
#[must_use]
pub fn plan_v3_outer_post_inner_step_effects() -> OuterPostInnerEffectPlan {
    OuterPostInnerEffectPlan {
        loop_guard_continuation: plan_outer_boundary_replay_effect(OuterBoundaryKind::LoopGuard),
        error_escalation_capacity_hold: true,
        in_turn_cycle_advance: plan_outer_boundary_replay_effect(
            OuterBoundaryKind::InTurnCycleAdvance,
        ),
    }
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
    fn outer_step_frame_plan_documents_reset_sync_cancel() {
        let plan = plan_v3_outer_step_frame_effects();
        assert!(plan.scratchpad_step_reset);
        assert!(plan.kernel_turn_frame_sync);
        assert!(plan.cancel_check);
    }

    #[test]
    fn outer_pre_inner_plan_documents_host_refresh_seam() {
        let plan = plan_v3_outer_pre_inner_step_effects();
        assert!(plan.live_steer_inject_per_message);
        assert_eq!(plan.system_prompt_refresh.effects.len(), 3);
        assert!(!plan.system_prompt_refresh.host_io_required);
        assert_eq!(plan.baseline.len(), 2);
    }

    #[test]
    fn live_steer_inject_effect_is_replay_aligned() {
        let effects = plan_live_steer_inject_effects(["steer-a".to_string()]);
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], Effect::InjectSteer { .. }));
    }

    #[test]
    fn outer_post_inner_plan_documents_conditional_boundaries() {
        let plan = plan_v3_outer_post_inner_step_effects();
        assert!(plan.error_escalation_capacity_hold);
        assert!(matches!(
            plan.loop_guard_continuation,
            Some(Effect::InjectSteer { .. })
        ));
        assert!(matches!(
            plan.in_turn_cycle_advance,
            Some(Effect::InjectSteer { .. })
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
