//! v3 capacity hold boundary planner logging (Phase 3b batch 5b cont.).

use zagens_core::capacity::GuardrailAction;
use zagens_core::engine::turn_loop::continuation_boundary_policy::OuterBoundaryKind;
use zagens_core::engine::turn_loop::live_turn_outer_planner::{
    CapacityCheckpointEffectTail, capacity_checkpoint_effect_tail_label,
    plan_capacity_hold_boundary_effect, verify_capacity_tail_alignment,
};

use super::*;

impl Engine {
    /// Log planned vs interpreted capacity hold tails when v3 outer loop holds.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::core::engine) fn log_v3_capacity_hold_planner_effect(
        &self,
        boundary: OuterBoundaryKind,
        turn_id: &str,
        step: u32,
        action: GuardrailAction,
        cooldown_blocked: bool,
        interpreted: CapacityCheckpointEffectTail,
        held: bool,
    ) {
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            return;
        }
        let planned = plan_capacity_hold_boundary_effect(action, cooldown_blocked);
        tracing::info!(
            target: "kernel_v3",
            turn_id = %turn_id,
            step,
            boundary = ?boundary,
            planned = capacity_checkpoint_effect_tail_label(planned),
            interpreted = capacity_checkpoint_effect_tail_label(interpreted),
            held,
            "v3 capacity hold planner effect (EffectInterpreter)"
        );
        if let Some(summary) = verify_capacity_tail_alignment(planned, interpreted) {
            tracing::warn!(
                target: "kernel_v3",
                %summary,
                boundary = ?boundary,
                "capacity hold planner tail mismatch"
            );
        }
    }
}
