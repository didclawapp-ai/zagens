//! Phase 3b batch 6g — v3 turn loop driver helpers.
//!
//! Capacity trim/handoff/cooldown/replay routing lives in
//! `capacity_flow::v3_routing` (batch 7d).

use crate::engine::kernel_mode::KernelMachineMode;
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
