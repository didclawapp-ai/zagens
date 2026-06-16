//! Pure outer-loop boundary policy (Phase 3b batch 5b).
//!
//! Encodes guard conditions from `run.rs` (step-limit, loop-guard, context
//! overflow, in-turn cycle advance) so live execution and replay projection
//! share one decision surface.

use crate::engine::context::MAX_CONTEXT_RECOVERY_ATTEMPTS;
use crate::engine::kernel_event::OverflowStrategy;
use crate::engine::streaming::{
    MAX_CONTEXT_CYCLE_HANDOFFS, MAX_IN_TURN_CYCLE_ADVANCES, MAX_LOOP_GUARD_CONTINUATIONS,
    MAX_STEP_LIMIT_CONTINUATIONS,
};
use crate::turn::{TurnContext, TurnLoopMode};

/// Which outer-loop boundary fired in the turn driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OuterBoundaryKind {
    StepLimit,
    LoopGuard,
    ContextOverflowCycleHandoff,
    InTurnCycleAdvance,
    /// Pre-request capacity intervention held the outer loop (`continue` without step advance).
    PreRequestCapacityHold,
    /// Error-escalation capacity intervention held the outer loop after tool errors.
    ErrorEscalationCapacityHold,
}

/// Outer-loop counters sampled at boundary checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OuterBoundaryCounters {
    pub step_limit_continuations: u32,
    pub loop_guard_continuations: u32,
    pub cycle_handoff_attempts: u32,
    pub in_turn_cycle_advances: u32,
}

/// Back-compat alias for continuation-only call sites.
pub type ContinuationBoundaryCounters = OuterBoundaryCounters;

/// Back-compat alias for continuation-only call sites.
pub type ContinuationBoundaryKind = OuterBoundaryKind;

/// Whether the step-limit cap may offer another bounded LHT grant.
#[must_use]
pub fn step_limit_boundary_eligible(mode: TurnLoopMode, counters: OuterBoundaryCounters) -> bool {
    !mode.is_plan() && counters.step_limit_continuations < MAX_STEP_LIMIT_CONTINUATIONS
}

/// Whether a loop-guard halt may offer another bounded LHT grant.
#[must_use]
pub fn loop_guard_boundary_eligible(
    mode: TurnLoopMode,
    loop_guard_halted: bool,
    counters: OuterBoundaryCounters,
) -> bool {
    loop_guard_halted
        && !mode.is_plan()
        && counters.loop_guard_continuations < MAX_LOOP_GUARD_CONTINUATIONS
}

/// Whether emergency compaction retries are exhausted for this overflow episode.
#[must_use]
pub fn context_recovery_attempts_exhausted(context_recovery_attempts: u8) -> bool {
    context_recovery_attempts >= MAX_CONTEXT_RECOVERY_ATTEMPTS
}

/// Whether a bounded cycle handoff may run after recovery exhaustion.
#[must_use]
pub fn cycle_handoff_boundary_eligible(
    mode: TurnLoopMode,
    counters: OuterBoundaryCounters,
) -> bool {
    !mode.is_plan() && counters.cycle_handoff_attempts < MAX_CONTEXT_CYCLE_HANDOFFS
}

/// Whether the clean in-turn cycle gate may run at a per-step safe boundary.
#[must_use]
pub fn in_turn_cycle_advance_boundary_eligible(
    mode: TurnLoopMode,
    counters: OuterBoundaryCounters,
) -> bool {
    !mode.is_plan() && counters.in_turn_cycle_advances < MAX_IN_TURN_CYCLE_ADVANCES
}

/// Reset recovery budget after a successful cycle handoff (fresh seed is small).
#[must_use]
pub const fn context_recovery_budget_after_cycle_handoff() -> u8 {
    0
}

/// User-visible failure when overflow persists after recovery + handoff grants.
#[must_use]
pub fn context_overflow_hard_fail_message(estimated_input: usize, input_budget: usize) -> String {
    format!(
        "Context remains above model limit after {} recovery attempts \
         (~{} token estimate, ~{} budget). Please run /compact or /clear.",
        MAX_CONTEXT_RECOVERY_ATTEMPTS, estimated_input, input_budget
    )
}

/// Kernel overflow strategy for a granted cycle handoff.
#[must_use]
pub const fn context_overflow_cycle_handoff_strategy() -> OverflowStrategy {
    OverflowStrategy::CycleHandoff
}

/// Kernel overflow strategy for budget recompile recovery.
#[must_use]
pub const fn context_overflow_budget_recompile_strategy() -> OverflowStrategy {
    OverflowStrategy::BudgetRecompile
}

/// New step budget after granting one step-limit continuation window.
#[must_use]
pub fn step_limit_budget_after_grant(turn: &TurnContext, step_budget_increment: u32) -> u32 {
    turn.max_steps.saturating_add(step_budget_increment)
}

#[must_use]
pub fn max_step_limit_grants() -> u32 {
    MAX_STEP_LIMIT_CONTINUATIONS
}

#[must_use]
pub fn max_loop_guard_grants() -> u32 {
    MAX_LOOP_GUARD_CONTINUATIONS
}

#[must_use]
pub fn max_context_cycle_handoffs() -> u32 {
    MAX_CONTEXT_CYCLE_HANDOFFS
}

#[must_use]
pub fn max_in_turn_cycle_advances() -> u32 {
    MAX_IN_TURN_CYCLE_ADVANCES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_limit_boundary_respects_plan_mode_and_cap() {
        let counters = OuterBoundaryCounters {
            step_limit_continuations: MAX_STEP_LIMIT_CONTINUATIONS,
            ..Default::default()
        };
        assert!(!step_limit_boundary_eligible(TurnLoopMode::Agent, counters));
        assert!(!step_limit_boundary_eligible(
            TurnLoopMode::Plan,
            Default::default()
        ));
        assert!(step_limit_boundary_eligible(
            TurnLoopMode::Agent,
            Default::default()
        ));
    }

    #[test]
    fn loop_guard_boundary_requires_halt_and_cap() {
        let at_cap = OuterBoundaryCounters {
            loop_guard_continuations: MAX_LOOP_GUARD_CONTINUATIONS,
            ..Default::default()
        };
        assert!(!loop_guard_boundary_eligible(
            TurnLoopMode::Agent,
            true,
            at_cap
        ));
        assert!(!loop_guard_boundary_eligible(
            TurnLoopMode::Agent,
            false,
            Default::default()
        ));
        assert!(loop_guard_boundary_eligible(
            TurnLoopMode::Agent,
            true,
            Default::default()
        ));
    }

    #[test]
    fn step_limit_budget_after_grant_adds_increment() {
        let turn = TurnContext::new(100);
        assert_eq!(step_limit_budget_after_grant(&turn, 100), 200);
    }

    #[test]
    fn context_recovery_exhaustion_matches_max() {
        assert!(!context_recovery_attempts_exhausted(0));
        assert!(!context_recovery_attempts_exhausted(
            MAX_CONTEXT_RECOVERY_ATTEMPTS.saturating_sub(1)
        ));
        assert!(context_recovery_attempts_exhausted(
            MAX_CONTEXT_RECOVERY_ATTEMPTS
        ));
    }

    #[test]
    fn cycle_handoff_boundary_respects_plan_and_cap() {
        let at_cap = OuterBoundaryCounters {
            cycle_handoff_attempts: MAX_CONTEXT_CYCLE_HANDOFFS,
            ..Default::default()
        };
        assert!(!cycle_handoff_boundary_eligible(
            TurnLoopMode::Plan,
            Default::default()
        ));
        assert!(!cycle_handoff_boundary_eligible(
            TurnLoopMode::Agent,
            at_cap
        ));
        assert!(cycle_handoff_boundary_eligible(
            TurnLoopMode::Agent,
            Default::default()
        ));
    }

    #[test]
    fn in_turn_cycle_advance_boundary_respects_cap() {
        let at_cap = OuterBoundaryCounters {
            in_turn_cycle_advances: MAX_IN_TURN_CYCLE_ADVANCES,
            ..Default::default()
        };
        assert!(!in_turn_cycle_advance_boundary_eligible(
            TurnLoopMode::Plan,
            Default::default()
        ));
        assert!(!in_turn_cycle_advance_boundary_eligible(
            TurnLoopMode::Agent,
            at_cap
        ));
        assert!(in_turn_cycle_advance_boundary_eligible(
            TurnLoopMode::Agent,
            Default::default()
        ));
    }
}
