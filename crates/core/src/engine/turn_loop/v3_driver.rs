//! Phase 3b batch 5b/6g — v3 turn loop driver helpers.
//!
//! Capacity trim/handoff/cooldown/replay routing lives in
//! `capacity_flow::v3_routing` (batch 7d).

use crate::engine::kernel_mode::KernelMachineMode;
use crate::engine::kernel_turn_host::KernelTurnHost;
use crate::engine::turn_loop::continuation_boundary_policy::{
    OuterBoundaryKind, max_context_cycle_handoffs, max_in_turn_cycle_advances,
    max_loop_guard_grants, max_step_limit_grants,
};
use crate::engine::turn_loop::live_turn_inner_planner::InnerStepEffectPlan;
use crate::engine::turn_loop::live_turn_outer_planner::plan_v3_pre_inner_step_baseline;
use crate::engine::turn_loop::live_turn_outer_planner::{
    OuterPostInnerEffectPlan, OuterPreInnerEffectPlan, OuterStepFrameEffectPlan,
};
use crate::engine::turn_loop::system_prompt_refresh_policy::SystemPromptRefreshPlan;
use crate::engine::turn_machine::Effect;
use crate::engine::turn_machine::plan_v3_step_effects;

use super::turn_loop_outer_host::OuterLoopHost;

/// Log when v3 refresh runs entirely through runtime ops (no direct outer-host call).
pub fn log_system_prompt_refresh_via_runtime_ops(turn_id: &str, step: u32) {
    tracing::debug!(
        target: "kernel_v3",
        turn_id = %turn_id,
        step,
        "v3 system prompt refresh complete (QueryMemory + assembly via runtime ops)"
    );
}

/// Log once per turn when the v3 effect-interpreter path is active.
pub fn log_v3_turn_start<H: KernelTurnHost>(host: &H, turn_id: &str) {
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
pub fn log_v3_outer_boundary<H: OuterLoopHost>(
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
pub fn log_v3_pre_inner_step_plan<H: KernelTurnHost>(host: &H, turn_id: &str, step: u32) {
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

/// Log inner-step baseline effect plan before CallModel IO (v3/shadow observability).
pub fn log_inner_step_effect_plan(turn_id: &str, step: u32, plan: &InnerStepEffectPlan) {
    tracing::debug!(
        target: "kernel_v3",
        turn_id = %turn_id,
        step,
        pre_call_count = plan.pre_call_model.len(),
        execute_batch_per_call = plan.execute_batch_per_call,
        notify_lsp_tail = plan.notify_lsp_tail,
        "v3 inner step effect plan (QueryMemory → CallModel → ExecuteBatch → NotifyLsp)"
    );
}

/// Log successful log-driven step-slice replay coherence (v3/shadow observability).
pub fn log_inner_step_slice_replay_ok(turn_id: &str, step: u32) {
    tracing::debug!(
        target: "kernel_v3",
        turn_id = %turn_id,
        step,
        "v3 inner step slice replay coherence ok (CallModel + ExecuteBatch + anchors)"
    );
}

/// Log successful log-driven `ModelRequestIssued` replay coherence (v3/shadow observability).
pub fn log_inner_step_model_request_replay_ok(turn_id: &str, step: u32) {
    tracing::debug!(
        target: "kernel_v3",
        turn_id = %turn_id,
        step,
        "v3 inner step ModelRequestIssued replay coherence ok (log-driven TurnMachine::step)"
    );
}

/// Log outer step-frame seam before pre-inner segment (v3/shadow observability).
pub fn log_outer_step_frame_plan(turn_id: &str, step: u32, plan: &OuterStepFrameEffectPlan) {
    tracing::debug!(
        target: "kernel_v3",
        turn_id = %turn_id,
        step,
        scratchpad_step_reset = plan.scratchpad_step_reset,
        kernel_turn_frame_sync = plan.kernel_turn_frame_sync,
        cancel_check = plan.cancel_check,
        "v3 outer step-frame plan (scratchpad reset + kernel sync + cancel gate)"
    );
}

/// Log the full outer pre-inner-step effect plan (v3/shadow observability).
pub fn log_outer_pre_inner_effect_plan(turn_id: &str, step: u32, plan: &OuterPreInnerEffectPlan) {
    tracing::debug!(
        target: "kernel_v3",
        turn_id = %turn_id,
        step,
        live_steer_inject_per_message = plan.live_steer_inject_per_message,
        system_prompt_query_effects = plan.system_prompt_refresh.effects.len(),
        system_prompt_host_io = plan.system_prompt_refresh.host_io_required,
        baseline_effects = plan.baseline.len(),
        "v3 outer pre-inner-step effect plan (system refresh seam + baseline IO)"
    );
}

/// Log system prompt refresh query plan before host IO (v3/shadow observability).
pub fn log_system_prompt_refresh_plan(turn_id: &str, step: u32, plan: &SystemPromptRefreshPlan) {
    tracing::debug!(
        target: "kernel_v3",
        turn_id = %turn_id,
        step,
        host_io_required = plan.host_io_required,
        refresh_effects = plan.effects.len(),
        "v3 system prompt refresh plan (QueryMemory + RefreshSystemPrompt)"
    );
}

/// Log live steer drain batch (v3/shadow observability).
pub fn log_live_steer_drain(turn_id: &str, step: u32, count: u32, effects: &[Effect]) {
    tracing::debug!(
        target: "kernel_v3",
        turn_id = %turn_id,
        step,
        steer_count = count,
        inject_steer_effects = effects.len(),
        "v3 live steer drain (rx_steer → InjectSteer)"
    );
}

/// Log conditional post-inner outer boundary slots (v3/shadow observability).
pub fn log_outer_post_inner_effect_plan(turn_id: &str, step: u32, plan: &OuterPostInnerEffectPlan) {
    tracing::debug!(
        target: "kernel_v3",
        turn_id = %turn_id,
        step,
        error_escalation_capacity_hold = plan.error_escalation_capacity_hold,
        loop_guard_continuation = plan.loop_guard_continuation.is_some(),
        in_turn_cycle_advance = plan.in_turn_cycle_advance.is_some(),
        "v3 outer post-inner-step effect plan (conditional boundary slots)"
    );
}
