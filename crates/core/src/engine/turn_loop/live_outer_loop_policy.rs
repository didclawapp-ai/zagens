//! Phase 3b batch 5d — pure outer-loop planning (TurnMachine-aligned substrate).
//!
//! Eligibility gates for `handle_deepseek_turn`. Host methods still perform IO;
//! this module owns the decision surface that folds into `TurnMachine::step`.

use crate::engine::turn_loop::continuation_boundary_policy::{
    OuterBoundaryCounters, context_overflow_hard_fail_message, context_recovery_attempts_exhausted,
    cycle_handoff_boundary_eligible, in_turn_cycle_advance_boundary_eligible,
    loop_guard_boundary_eligible, step_limit_boundary_eligible,
};
use crate::turn::TurnLoopMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepLimitDecision {
    NotAtCap,
    EligibleForHostContinue,
    TerminateAtCap,
}

#[must_use]
pub fn step_limit_decision(
    mode: TurnLoopMode,
    counters: OuterBoundaryCounters,
    at_max_steps: bool,
) -> StepLimitDecision {
    if !at_max_steps {
        return StepLimitDecision::NotAtCap;
    }
    if step_limit_boundary_eligible(mode, counters) {
        StepLimitDecision::EligibleForHostContinue
    } else {
        StepLimitDecision::TerminateAtCap
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextOverflowPreflightDecision {
    WithinBudget,
    TryBudgetRecompile,
    TryCycleHandoff,
    HardFail,
}

#[must_use]
pub fn context_overflow_preflight_decision(
    mode: TurnLoopMode,
    counters: OuterBoundaryCounters,
    context_recovery_attempts: u8,
    estimated_input: usize,
    input_budget: usize,
) -> ContextOverflowPreflightDecision {
    if estimated_input <= input_budget {
        return ContextOverflowPreflightDecision::WithinBudget;
    }
    if context_recovery_attempts_exhausted(context_recovery_attempts) {
        if cycle_handoff_boundary_eligible(mode, counters) {
            return ContextOverflowPreflightDecision::TryCycleHandoff;
        }
        return ContextOverflowPreflightDecision::HardFail;
    }
    ContextOverflowPreflightDecision::TryBudgetRecompile
}

#[must_use]
pub fn overflow_hard_fail_message(estimated_input: usize, input_budget: usize) -> String {
    context_overflow_hard_fail_message(estimated_input, input_budget)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopGuardHaltDecision {
    NotHalted,
    EligibleForHostContinue,
    Terminate,
}

#[must_use]
pub fn loop_guard_halt_decision(
    mode: TurnLoopMode,
    loop_guard_halted: bool,
    counters: OuterBoundaryCounters,
) -> LoopGuardHaltDecision {
    if !loop_guard_halted {
        return LoopGuardHaltDecision::NotHalted;
    }
    if loop_guard_boundary_eligible(mode, loop_guard_halted, counters) {
        LoopGuardHaltDecision::EligibleForHostContinue
    } else {
        LoopGuardHaltDecision::Terminate
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InTurnCycleAdvanceDecision {
    Skip,
    EligibleForHostAdvance,
}

#[must_use]
pub fn in_turn_cycle_advance_decision(
    mode: TurnLoopMode,
    counters: OuterBoundaryCounters,
) -> InTurnCycleAdvanceDecision {
    if in_turn_cycle_advance_boundary_eligible(mode, counters) {
        InTurnCycleAdvanceDecision::EligibleForHostAdvance
    } else {
        InTurnCycleAdvanceDecision::Skip
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::TurnLoopMode;

    #[test]
    fn step_limit_decision_at_cap_agent_eligible_for_host() {
        let counters = OuterBoundaryCounters::default();
        assert_eq!(
            step_limit_decision(TurnLoopMode::Agent, counters, true),
            StepLimitDecision::EligibleForHostContinue
        );
    }

    #[test]
    fn step_limit_decision_at_cap_plan_terminates() {
        let counters = OuterBoundaryCounters::default();
        assert_eq!(
            step_limit_decision(TurnLoopMode::Plan, counters, true),
            StepLimitDecision::TerminateAtCap
        );
    }

    #[test]
    fn overflow_preflight_within_budget() {
        let counters = OuterBoundaryCounters::default();
        assert_eq!(
            context_overflow_preflight_decision(TurnLoopMode::Agent, counters, 0, 100, 200),
            ContextOverflowPreflightDecision::WithinBudget
        );
    }

    #[test]
    fn loop_guard_halt_not_halted() {
        assert_eq!(
            loop_guard_halt_decision(TurnLoopMode::Agent, false, OuterBoundaryCounters::default()),
            LoopGuardHaltDecision::NotHalted
        );
    }
}
