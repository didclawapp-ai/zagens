//! Phase 3b batch 5b/6g — v3 turn loop driver helpers.
//!
//! Capacity trim/handoff/cooldown/replay routing lives in
//! `capacity_flow::v3_routing` (batch 7d).

use crate::engine::kernel_mode::KernelMachineMode;
use crate::engine::turn_loop::continuation_boundary_policy::{
    OuterBoundaryKind, max_context_cycle_handoffs, max_in_turn_cycle_advances,
    max_loop_guard_grants, max_step_limit_grants,
};
use crate::engine::turn_loop::live_turn_outer_planner::plan_v3_pre_inner_step_baseline;
use crate::engine::turn_machine::plan_v3_step_effects;

use super::host::TurnLoopHost;

/// Log once per turn when the v3 effect-interpreter path is active.
pub fn log_v3_turn_start<H: TurnLoopHost>(host: &H, turn_id: &str) {
    if host.kernel_machine_mode() == KernelMachineMode::V3 {
        tracing::info!(
            target: "kernel_v3",
            turn_id = %turn_id,
            "v3 turn loop active (CallModel / ExecuteBatch via effect interpreter)"
        );
    }
}

/// Log the planned effect chain for a v3 step (observability before IO).
///
/// `NotifyLsp` effects run after `ExecuteBatch` (effect interpreter or core fallback);
/// the turn loop skips the legacy pre-step `flush_pending_lsp_diagnostics` in v3 mode.
pub fn log_v3_step_effect_plan(turn_id: &str, step: u32, token_budget: u32, call_ids: &[String]) {
    let plan = plan_v3_step_effects(token_budget, call_ids);
    tracing::debug!(
        target: "kernel_v3",
        turn_id = %turn_id,
        step,
        effect_count = plan.len(),
        tool_count = call_ids.len(),
        "v3 step effect plan"
    );
}

/// Log when an outer-loop boundary grants another bounded window (v3 only).
pub fn log_v3_outer_boundary<H: TurnLoopHost>(
    host: &mut H,
    kind: OuterBoundaryKind,
    turn_id: &str,
    step: u32,
    grant_count: u32,
) {
    if host.kernel_machine_mode() != KernelMachineMode::V3 {
        return;
    }
    let (label, max_grants, message) = match kind {
        OuterBoundaryKind::StepLimit => (
            "step_limit",
            max_step_limit_grants(),
            "v3 outer boundary granted (TurnMachine-aligned event emitted)",
        ),
        OuterBoundaryKind::LoopGuard => (
            "loop_guard",
            max_loop_guard_grants(),
            "v3 outer boundary granted (TurnMachine-aligned event emitted)",
        ),
        OuterBoundaryKind::ContextOverflowCycleHandoff => (
            "context_overflow_cycle_handoff",
            max_context_cycle_handoffs(),
            "v3 outer boundary granted (TurnMachine-aligned event emitted)",
        ),
        OuterBoundaryKind::InTurnCycleAdvance => (
            "in_turn_cycle_advance",
            max_in_turn_cycle_advances(),
            "v3 outer boundary granted (TurnMachine-aligned event emitted)",
        ),
        OuterBoundaryKind::PreRequestCapacityHold => (
            "pre_request_capacity_hold",
            0,
            "v3 pre-request capacity hold (outer loop retry)",
        ),
        OuterBoundaryKind::ErrorEscalationCapacityHold => (
            "error_escalation_capacity_hold",
            0,
            "v3 error-escalation capacity hold (outer loop retry)",
        ),
    };
    tracing::info!(
        target: "kernel_v3",
        turn_id = %turn_id,
        step,
        boundary = label,
        grant_count,
        max_grants,
        message
    );
    host.record_v3_outer_boundary_grant(kind);
}

/// Log the baseline pre-inner-step effect plan before host IO (v3 only).
pub fn log_v3_pre_inner_step_plan<H: TurnLoopHost>(host: &H, turn_id: &str, step: u32) {
    if host.kernel_machine_mode() != KernelMachineMode::V3 {
        return;
    }
    let plan = plan_v3_pre_inner_step_baseline();
    tracing::debug!(
        target: "kernel_v3",
        turn_id = %turn_id,
        step,
        baseline_effects = plan.baseline.len(),
        "v3 pre-inner-step baseline effect plan (RunCompaction + RunLayeredContextCheckpoint)"
    );
}
