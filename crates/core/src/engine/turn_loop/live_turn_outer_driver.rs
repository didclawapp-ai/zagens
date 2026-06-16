//! Live outer-loop driver — gate wiring + grant records (Phase 3b batch 5b cont.).
//!
//! Policy lives in [`super::live_outer_loop_policy`]; replay effect labels in
//! [`super::live_turn_outer_planner`]. This module records counter/event updates
//! after host IO confirms a boundary grant, keeping `run.rs` aligned with
//! [`ReplayTurnMachine`](crate::engine::turn_machine::ReplayTurnMachine).

use crate::chat::LlmClient;
use crate::engine::kernel_event::{KernelEvent, OverflowStrategy};
use crate::engine::kernel_turn_host::KernelTurnHost;
use crate::engine::turn_loop::capacity_policy::should_run_capacity_error_escalation;
use crate::engine::turn_loop::continuation_boundary_policy::{
    OuterBoundaryCounters, OuterBoundaryKind, context_overflow_budget_recompile_strategy,
    context_overflow_cycle_handoff_strategy, context_recovery_budget_after_cycle_handoff,
    max_loop_guard_grants, max_step_limit_grants, step_limit_budget_after_grant,
};
use crate::engine::turn_loop::live_outer_loop_policy::{
    ContextOverflowPreflightDecision, InTurnCycleAdvanceDecision, LoopGuardHaltDecision,
    StepLimitDecision, context_overflow_preflight_decision, in_turn_cycle_advance_decision,
    loop_guard_halt_decision, overflow_hard_fail_message, step_limit_decision,
};
use crate::engine::turn_loop::live_turn_outer_planner::plan_outer_boundary_replay_effect;
use crate::error_taxonomy::ErrorCategory;
use crate::turn::{TurnContext, TurnLoopMode};

use super::turn_loop_outer_host::OuterLoopHost;

/// User-visible status line emitted on an outer boundary grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBroadcast {
    pub message: String,
}

/// Counter/event bundle applied after host confirms an outer boundary grant.
#[derive(Debug, Clone)]
pub struct OuterBoundaryGrant {
    pub counters: OuterBoundaryCounters,
    pub kernel_event: Option<KernelEvent>,
    pub status: Option<StatusBroadcast>,
    pub boundary_kind: Option<OuterBoundaryKind>,
    pub context_recovery_attempts: Option<u8>,
    pub turn_max_steps: Option<u32>,
}

/// Verify a live grant matches replay-aligned boundary expectations (TurnMachine substrate).
#[must_use]
pub fn verify_outer_boundary_grant_replay_coherence(grant: &OuterBoundaryGrant) -> Option<String> {
    let Some(kind) = grant.boundary_kind else {
        return None;
    };
    let planned_effect = plan_outer_boundary_replay_effect(kind);
    match planned_effect {
        None => {
            if grant.kernel_event.is_some() {
                return Some(format!(
                    "capacity hold {kind:?} must not emit boundary kernel event"
                ));
            }
            None
        }
        Some(_) => {
            if !grant_kernel_event_matches_boundary(kind, grant.kernel_event.as_ref()) {
                return Some(format!(
                    "boundary {kind:?} kernel_event mismatch (got {:?})",
                    grant
                        .kernel_event
                        .as_ref()
                        .map(|event| format!("{event:?}"))
                ));
            }
            None
        }
    }
}

fn grant_kernel_event_matches_boundary(
    kind: OuterBoundaryKind,
    event: Option<&KernelEvent>,
) -> bool {
    match kind {
        OuterBoundaryKind::StepLimit => {
            matches!(event, Some(KernelEvent::StepLimitContinuation { .. }))
        }
        OuterBoundaryKind::LoopGuard => {
            matches!(event, Some(KernelEvent::LoopGuardContinuation { .. }))
        }
        OuterBoundaryKind::ContextOverflowCycleHandoff => matches!(
            event,
            Some(KernelEvent::ContextOverflowRecovered {
                strategy: OverflowStrategy::CycleHandoff,
                ..
            })
        ),
        OuterBoundaryKind::InTurnCycleAdvance => event.is_none(),
        OuterBoundaryKind::PreRequestCapacityHold
        | OuterBoundaryKind::ErrorEscalationCapacityHold => event.is_none(),
    }
}

/// Grant ordinal for v3 outer-boundary logging (`log_v3_outer_boundary`).
#[must_use]
pub fn outer_boundary_grant_log_count(grant: &OuterBoundaryGrant) -> u32 {
    match grant.boundary_kind {
        Some(OuterBoundaryKind::StepLimit) => grant.counters.step_limit_continuations,
        Some(OuterBoundaryKind::LoopGuard) => grant.counters.loop_guard_continuations,
        Some(OuterBoundaryKind::ContextOverflowCycleHandoff) => {
            grant.counters.cycle_handoff_attempts
        }
        Some(OuterBoundaryKind::InTurnCycleAdvance) => grant.counters.in_turn_cycle_advances,
        Some(OuterBoundaryKind::PreRequestCapacityHold)
        | Some(OuterBoundaryKind::ErrorEscalationCapacityHold)
        | None => 0,
    }
}

/// Pre-inner step-limit gate (pure policy surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreInnerStepLimitGate {
    NotAtCap,
    AwaitHostContinue,
    TerminateAtCap,
}

/// Pre-inner overflow preflight gate (pure policy surface).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreInnerOverflowGate {
    WithinBudget,
    AwaitBudgetRecompile,
    AwaitCycleHandoff,
    HardFail,
}

/// Post-inner loop-guard gate (pure policy surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostInnerLoopGuardGate {
    NotHalted,
    AwaitHostContinue,
    Terminate,
}

/// Post-inner in-turn cycle advance gate (pure policy surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostInnerCycleAdvanceGate {
    Skip,
    AwaitHostAdvance,
}

#[must_use]
pub fn pre_inner_step_limit_gate(
    mode: TurnLoopMode,
    counters: OuterBoundaryCounters,
    at_max_steps: bool,
) -> PreInnerStepLimitGate {
    match step_limit_decision(mode, counters, at_max_steps) {
        StepLimitDecision::NotAtCap => PreInnerStepLimitGate::NotAtCap,
        StepLimitDecision::EligibleForHostContinue => PreInnerStepLimitGate::AwaitHostContinue,
        StepLimitDecision::TerminateAtCap => PreInnerStepLimitGate::TerminateAtCap,
    }
}

#[must_use]
pub fn pre_inner_overflow_gate(
    mode: TurnLoopMode,
    counters: OuterBoundaryCounters,
    context_recovery_attempts: u8,
    estimated_input: usize,
    input_budget: usize,
) -> PreInnerOverflowGate {
    match context_overflow_preflight_decision(
        mode,
        counters,
        context_recovery_attempts,
        estimated_input,
        input_budget,
    ) {
        ContextOverflowPreflightDecision::WithinBudget => PreInnerOverflowGate::WithinBudget,
        ContextOverflowPreflightDecision::TryBudgetRecompile => {
            PreInnerOverflowGate::AwaitBudgetRecompile
        }
        ContextOverflowPreflightDecision::TryCycleHandoff => {
            PreInnerOverflowGate::AwaitCycleHandoff
        }
        ContextOverflowPreflightDecision::HardFail => PreInnerOverflowGate::HardFail,
    }
}

#[must_use]
pub fn overflow_hard_fail_user_message(estimated_input: usize, input_budget: usize) -> String {
    overflow_hard_fail_message(estimated_input, input_budget)
}

#[must_use]
pub fn post_inner_loop_guard_gate(
    mode: TurnLoopMode,
    loop_guard_halted: bool,
    counters: OuterBoundaryCounters,
) -> PostInnerLoopGuardGate {
    match loop_guard_halt_decision(mode, loop_guard_halted, counters) {
        LoopGuardHaltDecision::NotHalted => PostInnerLoopGuardGate::NotHalted,
        LoopGuardHaltDecision::EligibleForHostContinue => PostInnerLoopGuardGate::AwaitHostContinue,
        LoopGuardHaltDecision::Terminate => PostInnerLoopGuardGate::Terminate,
    }
}

#[must_use]
pub fn post_inner_cycle_advance_gate(
    mode: TurnLoopMode,
    counters: OuterBoundaryCounters,
) -> PostInnerCycleAdvanceGate {
    match in_turn_cycle_advance_decision(mode, counters) {
        InTurnCycleAdvanceDecision::Skip => PostInnerCycleAdvanceGate::Skip,
        InTurnCycleAdvanceDecision::EligibleForHostAdvance => {
            PostInnerCycleAdvanceGate::AwaitHostAdvance
        }
    }
}

#[must_use]
pub fn apply_step_limit_continuation(
    turn: &TurnContext,
    counters: OuterBoundaryCounters,
    step_budget_increment: u32,
) -> OuterBoundaryGrant {
    let new_count = counters.step_limit_continuations.saturating_add(1);
    OuterBoundaryGrant {
        counters: OuterBoundaryCounters {
            step_limit_continuations: new_count,
            ..counters
        },
        kernel_event: Some(KernelEvent::StepLimitContinuation {
            turn_id: turn.id.clone(),
            step_idx: turn.step,
            lht_objective_injected: true,
        }),
        status: Some(StatusBroadcast {
            message: format!(
                "Step budget reached; continuing long-horizon task ({}/{})",
                new_count,
                max_step_limit_grants()
            ),
        }),
        boundary_kind: Some(OuterBoundaryKind::StepLimit),
        context_recovery_attempts: None,
        turn_max_steps: Some(step_limit_budget_after_grant(turn, step_budget_increment)),
    }
}

#[must_use]
pub fn apply_loop_guard_continuation(
    turn: &TurnContext,
    counters: OuterBoundaryCounters,
) -> OuterBoundaryGrant {
    let new_count = counters.loop_guard_continuations.saturating_add(1);
    OuterBoundaryGrant {
        counters: OuterBoundaryCounters {
            loop_guard_continuations: new_count,
            ..counters
        },
        kernel_event: Some(KernelEvent::LoopGuardContinuation {
            turn_id: turn.id.clone(),
            step_idx: turn.step,
        }),
        status: Some(StatusBroadcast {
            message: format!(
                "Loop-guard halt; nudging long-horizon task to change approach ({}/{})",
                new_count,
                max_loop_guard_grants()
            ),
        }),
        boundary_kind: Some(OuterBoundaryKind::LoopGuard),
        context_recovery_attempts: None,
        turn_max_steps: None,
    }
}

#[must_use]
pub fn apply_context_overflow_cycle_handoff(
    turn: &TurnContext,
    counters: OuterBoundaryCounters,
    input_budget: u32,
) -> OuterBoundaryGrant {
    let new_count = counters.cycle_handoff_attempts.saturating_add(1);
    OuterBoundaryGrant {
        counters: OuterBoundaryCounters {
            cycle_handoff_attempts: new_count,
            ..counters
        },
        kernel_event: Some(KernelEvent::ContextOverflowRecovered {
            turn_id: turn.id.clone(),
            step_idx: turn.step,
            strategy: context_overflow_cycle_handoff_strategy(),
            source_budget_cap: Some(input_budget),
        }),
        status: None,
        boundary_kind: Some(OuterBoundaryKind::ContextOverflowCycleHandoff),
        context_recovery_attempts: Some(context_recovery_budget_after_cycle_handoff()),
        turn_max_steps: None,
    }
}

#[must_use]
pub fn apply_context_overflow_budget_recompile(
    turn: &TurnContext,
    counters: OuterBoundaryCounters,
    context_recovery_attempts: u8,
    input_budget: u32,
) -> OuterBoundaryGrant {
    OuterBoundaryGrant {
        counters,
        kernel_event: Some(KernelEvent::ContextOverflowRecovered {
            turn_id: turn.id.clone(),
            step_idx: turn.step,
            strategy: context_overflow_budget_recompile_strategy(),
            source_budget_cap: Some(input_budget),
        }),
        status: None,
        boundary_kind: None,
        context_recovery_attempts: Some(context_recovery_attempts.saturating_add(1)),
        turn_max_steps: None,
    }
}

#[must_use]
pub fn apply_in_turn_cycle_advance(counters: OuterBoundaryCounters) -> OuterBoundaryGrant {
    let new_count = counters.in_turn_cycle_advances.saturating_add(1);
    OuterBoundaryGrant {
        counters: OuterBoundaryCounters {
            in_turn_cycle_advances: new_count,
            ..counters
        },
        kernel_event: None,
        status: None,
        boundary_kind: Some(OuterBoundaryKind::InTurnCycleAdvance),
        context_recovery_attempts: None,
        turn_max_steps: None,
    }
}

/// Run pre-inner-step baseline IO (v3: `EffectInterpreter` plan; legacy: host hooks).
pub async fn run_pre_inner_step_baseline<H: OuterLoopHost>(
    host: &mut H,
    client: &dyn LlmClient,
    turn: &TurnContext,
) {
    if host.kernel_machine_mode().uses_v3_turn_loop()
        && KernelTurnHost::try_run_pre_inner_step_baseline(host, client, turn).await
    {
        return;
    }
    host.run_pre_inner_step_auto_compaction(client, turn).await;
    host.run_pre_inner_step_layered_context().await;
}

/// Post-inner error-escalation capacity gate (pure policy surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostInnerErrorEscalationGate {
    Skip,
    RunCheckpoint,
}

#[must_use]
pub fn post_inner_error_escalation_gate(
    step_error_count: usize,
    consecutive_tool_error_steps: u32,
    error_categories: &[ErrorCategory],
) -> PostInnerErrorEscalationGate {
    if should_run_capacity_error_escalation(
        step_error_count,
        consecutive_tool_error_steps,
        error_categories,
    ) {
        PostInnerErrorEscalationGate::RunCheckpoint
    } else {
        PostInnerErrorEscalationGate::Skip
    }
}

fn log_capacity_hold_boundary<H: OuterLoopHost>(
    host: &mut H,
    kind: OuterBoundaryKind,
    turn: &TurnContext,
) {
    super::v3_driver::log_v3_outer_boundary(host, kind, &turn.id, turn.step, turn.step);
}

/// Pre-request capacity hold: checkpoint IO + v3 outer-boundary observability when held.
pub async fn run_capacity_pre_request_hold<H: OuterLoopHost>(
    host: &mut H,
    turn: &TurnContext,
    client: Option<&dyn LlmClient>,
    mode: TurnLoopMode,
) -> bool {
    let held = host
        .run_capacity_pre_request_checkpoint(turn, client, mode)
        .await;
    if held {
        log_capacity_hold_boundary(host, OuterBoundaryKind::PreRequestCapacityHold, turn);
    }
    held
}

/// Post-tool error-escalation capacity hold: policy gate + checkpoint IO + observability.
pub async fn run_capacity_error_escalation_hold<H: OuterLoopHost>(
    host: &mut H,
    turn: &mut TurnContext,
    mode: TurnLoopMode,
    step_error_count: usize,
    consecutive_tool_error_steps: u32,
    error_categories: &[ErrorCategory],
) -> bool {
    if matches!(
        post_inner_error_escalation_gate(
            step_error_count,
            consecutive_tool_error_steps,
            error_categories,
        ),
        PostInnerErrorEscalationGate::Skip
    ) {
        return false;
    }
    let held = host
        .run_capacity_error_escalation_checkpoint(
            turn,
            mode,
            step_error_count,
            consecutive_tool_error_steps,
            error_categories,
        )
        .await;
    if held {
        log_capacity_hold_boundary(host, OuterBoundaryKind::ErrorEscalationCapacityHold, turn);
    }
    held
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::kernel_event::OverflowStrategy;

    fn sample_turn() -> TurnContext {
        let mut turn = TurnContext::new(8);
        turn.id = "t1".into();
        turn.step = 2;
        turn
    }

    #[test]
    fn step_limit_gate_maps_policy() {
        let counters = OuterBoundaryCounters::default();
        assert_eq!(
            pre_inner_step_limit_gate(TurnLoopMode::Agent, counters, false),
            PreInnerStepLimitGate::NotAtCap
        );
        assert_eq!(
            pre_inner_step_limit_gate(TurnLoopMode::Plan, counters, true),
            PreInnerStepLimitGate::TerminateAtCap
        );
    }

    #[test]
    fn apply_step_limit_grant_emits_event_and_budget() {
        let turn = sample_turn();
        let grant = apply_step_limit_continuation(&turn, OuterBoundaryCounters::default(), 8);
        assert_eq!(grant.counters.step_limit_continuations, 1);
        assert_eq!(grant.turn_max_steps, Some(16));
        assert!(matches!(
            grant.kernel_event,
            Some(KernelEvent::StepLimitContinuation { .. })
        ));
    }

    #[test]
    fn overflow_gate_hard_fail_when_exhausted() {
        let counters = OuterBoundaryCounters::default();
        assert_eq!(
            pre_inner_overflow_gate(TurnLoopMode::Plan, counters, u8::MAX, 500, 100),
            PreInnerOverflowGate::HardFail
        );
    }

    #[test]
    fn cycle_handoff_grant_resets_recovery_budget() {
        let turn = sample_turn();
        let grant =
            apply_context_overflow_cycle_handoff(&turn, OuterBoundaryCounters::default(), 128_000);
        assert_eq!(grant.counters.cycle_handoff_attempts, 1);
        assert_eq!(grant.context_recovery_attempts, Some(0));
        assert!(matches!(
            grant.kernel_event,
            Some(KernelEvent::ContextOverflowRecovered {
                strategy: OverflowStrategy::CycleHandoff,
                ..
            })
        ));
    }

    #[test]
    fn loop_guard_grant_increments_counter() {
        let turn = sample_turn();
        let grant = apply_loop_guard_continuation(&turn, OuterBoundaryCounters::default());
        assert_eq!(grant.counters.loop_guard_continuations, 1);
        assert_eq!(grant.boundary_kind, Some(OuterBoundaryKind::LoopGuard));
    }

    #[test]
    fn error_escalation_gate_skips_clean_step() {
        assert_eq!(
            post_inner_error_escalation_gate(0, 0, &[]),
            PostInnerErrorEscalationGate::Skip
        );
    }

    #[test]
    fn error_escalation_gate_runs_on_overflow_category() {
        assert_eq!(
            post_inner_error_escalation_gate(1, 0, &[ErrorCategory::InvalidInput]),
            PostInnerErrorEscalationGate::RunCheckpoint
        );
    }

    #[test]
    fn grant_replay_coherence_accepts_step_limit_grant() {
        let turn = sample_turn();
        let grant = apply_step_limit_continuation(&turn, OuterBoundaryCounters::default(), 8);
        assert!(verify_outer_boundary_grant_replay_coherence(&grant).is_none());
    }

    #[test]
    fn grant_replay_coherence_rejects_step_limit_without_event() {
        let grant = OuterBoundaryGrant {
            counters: OuterBoundaryCounters::default(),
            kernel_event: None,
            status: None,
            boundary_kind: Some(OuterBoundaryKind::StepLimit),
            context_recovery_attempts: None,
            turn_max_steps: None,
        };
        assert!(verify_outer_boundary_grant_replay_coherence(&grant).is_some());
    }

    #[test]
    fn grant_replay_coherence_accepts_in_turn_cycle_advance_without_event() {
        let grant = apply_in_turn_cycle_advance(OuterBoundaryCounters::default());
        assert!(verify_outer_boundary_grant_replay_coherence(&grant).is_none());
    }
}
