//! Live outer-loop driver — `TurnMachine` planning surface for `handle_deepseek_turn` (batch 5b cont.).
//!
//! [`LiveTurnMachine`] delegates kernel-event replay to [`ReplayTurnMachine`] and centralizes
//! outer-loop gate + effect planning. Inner-step live IO follows [`inner_step_live_plan`](Self::inner_step_live_plan)
//! through runtime [`EffectInterpreter`] (batch 5 closure).

use std::collections::HashSet;

use crate::chat::{ContentBlock, LlmClient, Message, Tool};
use crate::engine::context::{TURN_MAX_OUTPUT_TOKENS, context_input_budget};
use crate::engine::kernel_event::KernelEvent;
use crate::engine::loop_guard::LoopGuard;
use crate::engine::turn_loop::continuation_boundary_policy::{
    OuterBoundaryCounters, OuterBoundaryKind,
};
use crate::engine::turn_loop::control::TurnLoopToolPhaseOutcome;
use crate::engine::turn_loop::inner_step_replay_policy::{
    model_request_issued_for_step, verify_inner_step_model_request_replay_coherence,
    verify_inner_step_slice_replay_coherence,
};
use crate::engine::turn_loop::live_turn_inner_driver::{
    inner_step_execute_batch_effects, inner_step_notify_lsp_effects, plan_inner_step_live_effects,
};
use crate::engine::turn_loop::live_turn_outer_driver::{
    OuterBoundaryGrant, PostInnerCycleAdvanceGate, PostInnerErrorEscalationGate,
    PostInnerLoopGuardGate, PreInnerOverflowGate, PreInnerStepLimitGate,
    apply_context_overflow_budget_recompile, apply_context_overflow_cycle_handoff,
    apply_in_turn_cycle_advance, apply_loop_guard_continuation, apply_step_limit_continuation,
    outer_boundary_grant_log_count, overflow_hard_fail_user_message, post_inner_cycle_advance_gate,
    post_inner_error_escalation_gate, post_inner_loop_guard_gate, pre_inner_overflow_gate,
    pre_inner_step_limit_gate, run_capacity_error_escalation_hold, run_capacity_pre_request_hold,
    run_pre_inner_step_baseline, verify_outer_boundary_grant_replay_coherence,
};
use crate::engine::turn_loop::live_turn_outer_planner::{
    OuterPostInnerEffectPlan, OuterPreInnerEffectPlan, OuterStepFrameEffectPlan,
    plan_live_steer_inject_effects, plan_outer_boundary_replay_effect,
    plan_v3_outer_post_inner_step_effects, plan_v3_outer_pre_inner_step_effects,
    plan_v3_outer_step_frame_effects, plan_v3_pre_inner_step_baseline,
    verify_outer_boundary_effect_alignment,
};
use crate::engine::turn_machine::{
    Effect, LiveTurnSnapshot, ReplayTurnMachine, StepOutput, TurnKernelProjection, TurnMachine,
    emit_kernel_event,
};
use crate::error_taxonomy::{ErrorCategory, ErrorEnvelope};
use crate::events::Event;
use crate::turn::{TurnContext, TurnLoopMode};

use super::host::V3TurnHost;
use super::turn_loop_outer_host::OuterLoopHost;
use super::v3_step::{V3StepOutcome, run_v3_turn_step_unified};

/// Outcome of the outer step-frame segment (per-iteration reset/sync/cancel before pre-inner).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OuterStepFrameOutcome {
    Proceed,
    Cancelled,
}

/// Outcome of the outer pre-inner-step segment (before streaming/tool IO).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OuterPreInnerStepOutcome {
    ContinueOuterLoop,
    BreakOuterLoop,
    Failed,
    ProceedToInnerStep,
}

/// Outcome of the outer post-inner-step segment (after streaming/tool IO).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OuterPostInnerStepOutcome {
    ContinueOuterLoop,
    BreakOuterLoop,
    AdvanceStep,
}

/// Mutable outer-loop counters and per-turn driver scratch state.
#[derive(Debug, Default)]
pub struct LiveOuterLoopState {
    pub step_limit_continuations: u32,
    pub loop_guard_continuations: u32,
    pub cycle_handoff_attempts: u32,
    pub in_turn_cycle_advances: u32,
    pub context_recovery_attempts: u8,
    pub consecutive_tool_error_steps: u32,
    pub stream_retry_attempts: u32,
    pub length_continuations: u32,
    pub turn_error: Option<String>,
}

impl LiveOuterLoopState {
    #[must_use]
    pub fn boundary_counters(&self) -> OuterBoundaryCounters {
        OuterBoundaryCounters {
            step_limit_continuations: self.step_limit_continuations,
            loop_guard_continuations: self.loop_guard_continuations,
            cycle_handoff_attempts: self.cycle_handoff_attempts,
            in_turn_cycle_advances: self.in_turn_cycle_advances,
        }
    }

    pub fn apply_grant(&mut self, grant: &OuterBoundaryGrant, turn: &mut TurnContext) {
        self.step_limit_continuations = grant.counters.step_limit_continuations;
        self.loop_guard_continuations = grant.counters.loop_guard_continuations;
        self.cycle_handoff_attempts = grant.counters.cycle_handoff_attempts;
        self.in_turn_cycle_advances = grant.counters.in_turn_cycle_advances;
        if let Some(max_steps) = grant.turn_max_steps {
            turn.max_steps = max_steps;
        }
        if let Some(attempts) = grant.context_recovery_attempts {
            self.context_recovery_attempts = attempts;
        }
    }

    #[must_use]
    pub fn live_snapshot(
        &self,
        turn: &TurnContext,
        scratchpad_summary_injected: bool,
    ) -> LiveTurnSnapshot {
        LiveTurnSnapshot {
            turn_id: turn.id.clone(),
            step_idx: turn.step,
            max_steps: turn.max_steps,
            scratchpad_summary_injected,
            step_limit_continuations: self.step_limit_continuations,
            loop_guard_continuations: self.loop_guard_continuations,
            cycle_handoff_attempts: self.cycle_handoff_attempts,
            in_turn_cycle_advances: self.in_turn_cycle_advances,
        }
    }
}

/// Live turn driver: replay parity + outer-loop gate/effect planning.
#[derive(Debug, Default)]
pub struct LiveTurnMachine {
    replay: ReplayTurnMachine,
}

impl TurnMachine for LiveTurnMachine {
    fn step(&mut self, projection: &TurnKernelProjection, event: KernelEvent) -> StepOutput {
        self.replay.step(projection, event)
    }
}

impl LiveTurnMachine {
    #[must_use]
    pub fn pre_inner_baseline_effects(&self) -> Vec<Effect> {
        plan_v3_pre_inner_step_baseline().baseline
    }

    #[must_use]
    pub fn inner_step_baseline_plan(
        &self,
        projection: &TurnKernelProjection,
        token_budget: u32,
    ) -> crate::engine::turn_loop::live_turn_inner_planner::InnerStepEffectPlan {
        plan_inner_step_live_effects(projection, token_budget, None).baseline
    }

    /// Full live inner-step plan (baseline + documented dynamic tail flags).
    #[must_use]
    pub fn inner_step_live_plan(
        &self,
        projection: &TurnKernelProjection,
        token_budget: u32,
        episodic_hints: Option<
            crate::engine::turn_loop::memory_plane_episodic_policy::MemoryPlaneEpisodicHints,
        >,
    ) -> crate::engine::turn_loop::live_turn_inner_driver::InnerStepLiveEffectPlan {
        let _ = self;
        plan_inner_step_live_effects(projection, token_budget, episodic_hints)
    }

    /// Post-`CallModel` `ExecuteBatch` effects (machine-planned tail).
    #[must_use]
    pub fn inner_step_execute_batch_effects(
        &self,
        token_budget: u32,
        call_ids: &[String],
    ) -> Vec<Effect> {
        let _ = self;
        inner_step_execute_batch_effects(token_budget, call_ids)
    }

    /// Post-`ExecuteBatch` `NotifyLsp` tail from step events.
    #[must_use]
    pub fn inner_step_notify_lsp_effects(&self, step_events: &[KernelEvent]) -> Vec<Effect> {
        let _ = self;
        inner_step_notify_lsp_effects(step_events)
    }

    /// Post-IO: verify observed `ModelRequestIssued` replay against inner-step planner.
    #[must_use]
    pub fn verify_inner_step_from_turn_log(
        &self,
        turn_events: &[KernelEvent],
        step_idx: u32,
    ) -> Option<String> {
        let _ = self;
        verify_inner_step_model_request_replay_coherence(turn_events, step_idx)
    }

    /// Post-IO: verify full step-slice effect replay from the turn log.
    #[must_use]
    pub fn verify_inner_step_slice_from_turn_log(
        &self,
        turn_events: &[KernelEvent],
        step_idx: u32,
    ) -> Option<String> {
        let _ = self;
        verify_inner_step_slice_replay_coherence(turn_events, step_idx)
    }

    #[must_use]
    pub fn outer_step_frame_effect_plan(&self) -> OuterStepFrameEffectPlan {
        plan_v3_outer_step_frame_effects()
    }

    #[must_use]
    pub fn outer_pre_inner_effect_plan(&self) -> OuterPreInnerEffectPlan {
        plan_v3_outer_pre_inner_step_effects()
    }

    #[must_use]
    pub fn outer_post_inner_effect_plan(&self) -> OuterPostInnerEffectPlan {
        plan_v3_outer_post_inner_step_effects()
    }

    #[must_use]
    pub fn boundary_confirmation_effects(&self, kind: OuterBoundaryKind) -> Vec<Effect> {
        plan_outer_boundary_replay_effect(kind)
            .into_iter()
            .collect()
    }

    /// Check grant record + replay effect plan alignment (v3 observability).
    #[must_use]
    pub fn verify_boundary_grant(&self, grant: &OuterBoundaryGrant) -> Option<String> {
        if let Some(summary) = verify_outer_boundary_grant_replay_coherence(grant) {
            return Some(summary);
        }
        let kind = grant.boundary_kind?;
        let planned = plan_outer_boundary_replay_effect(kind);
        let confirmation = self.boundary_confirmation_effects(kind);
        match (planned, confirmation.as_slice()) {
            (None, []) => None,
            (Some(_), [Effect::InjectSteer { .. }]) => {
                verify_outer_boundary_effect_alignment(kind, confirmation.first().cloned())
            }
            (Some(p), _) => Some(format!(
                "boundary {kind:?} confirmation effects mismatch planned={p:?} got={confirmation:?}"
            )),
            (None, _) => Some(format!(
                "boundary {kind:?} has confirmation effects but no replay plan"
            )),
        }
    }

    #[must_use]
    pub fn pre_inner_step_limit_gate(
        &self,
        mode: TurnLoopMode,
        counters: OuterBoundaryCounters,
        at_max_steps: bool,
    ) -> PreInnerStepLimitGate {
        pre_inner_step_limit_gate(mode, counters, at_max_steps)
    }

    #[must_use]
    pub fn pre_inner_overflow_gate(
        &self,
        mode: TurnLoopMode,
        counters: OuterBoundaryCounters,
        context_recovery_attempts: u8,
        estimated_input: usize,
        input_budget: usize,
    ) -> PreInnerOverflowGate {
        pre_inner_overflow_gate(
            mode,
            counters,
            context_recovery_attempts,
            estimated_input,
            input_budget,
        )
    }

    #[must_use]
    pub fn overflow_hard_fail_message(
        &self,
        estimated_input: usize,
        input_budget: usize,
    ) -> String {
        overflow_hard_fail_user_message(estimated_input, input_budget)
    }

    #[must_use]
    pub fn post_inner_loop_guard_gate(
        &self,
        mode: TurnLoopMode,
        loop_guard_halted: bool,
        counters: OuterBoundaryCounters,
    ) -> PostInnerLoopGuardGate {
        post_inner_loop_guard_gate(mode, loop_guard_halted, counters)
    }

    #[must_use]
    pub fn post_inner_cycle_advance_gate(
        &self,
        mode: TurnLoopMode,
        counters: OuterBoundaryCounters,
    ) -> PostInnerCycleAdvanceGate {
        post_inner_cycle_advance_gate(mode, counters)
    }

    pub fn step_limit_continuation_grant(
        &self,
        turn: &TurnContext,
        counters: OuterBoundaryCounters,
        step_budget_increment: u32,
    ) -> OuterBoundaryGrant {
        apply_step_limit_continuation(turn, counters, step_budget_increment)
    }

    pub fn loop_guard_continuation_grant(
        &self,
        turn: &TurnContext,
        counters: OuterBoundaryCounters,
    ) -> OuterBoundaryGrant {
        apply_loop_guard_continuation(turn, counters)
    }

    pub fn overflow_cycle_handoff_grant(
        &self,
        turn: &TurnContext,
        counters: OuterBoundaryCounters,
        input_budget: u32,
    ) -> OuterBoundaryGrant {
        apply_context_overflow_cycle_handoff(turn, counters, input_budget)
    }

    pub fn overflow_budget_recompile_grant(
        &self,
        turn: &TurnContext,
        counters: OuterBoundaryCounters,
        context_recovery_attempts: u8,
        input_budget: u32,
    ) -> OuterBoundaryGrant {
        apply_context_overflow_budget_recompile(
            turn,
            counters,
            context_recovery_attempts,
            input_budget,
        )
    }

    pub fn in_turn_cycle_advance_grant(
        &self,
        counters: OuterBoundaryCounters,
    ) -> OuterBoundaryGrant {
        apply_in_turn_cycle_advance(counters)
    }
}

pub async fn drain_live_steers_via_machine<H: OuterLoopHost>(
    host: &mut H,
    turn: &TurnContext,
    _machine: &LiveTurnMachine,
) -> u32 {
    let mut steers = Vec::new();
    while let Ok(steer) = host.rx_steer_mut().try_recv() {
        steers.push(steer.clone());
        host.inject_live_steer(turn, steer).await;
    }
    let count = u32::try_from(steers.len()).unwrap_or(u32::MAX);
    if host.kernel_machine_mode().uses_v3_turn_loop() && count > 0 {
        super::v3_driver::log_live_steer_drain(
            &turn.id,
            turn.step,
            count,
            &plan_live_steer_inject_effects(steers),
        );
    }
    count
}

pub async fn run_outer_step_frame_via_machine<H: OuterLoopHost>(
    host: &mut H,
    turn: &TurnContext,
    machine: &LiveTurnMachine,
) -> OuterStepFrameOutcome {
    if host.kernel_machine_mode().uses_v3_turn_loop() {
        let plan = machine.outer_step_frame_effect_plan();
        super::v3_driver::log_outer_step_frame_plan(&turn.id, turn.step, &plan);
    }
    host.reset_scratchpad_step();
    host.sync_kernel_turn_frame(turn);
    if host.cancel_token().is_cancelled() {
        let _ = host
            .tx_event()
            .send(Event::status("Request cancelled"))
            .await;
        return OuterStepFrameOutcome::Cancelled;
    }
    OuterStepFrameOutcome::Proceed
}

/// Run one inner step (CallModel + optional ExecuteBatch) via v3 interpreter with machine planning.
#[allow(clippy::too_many_arguments)]
pub async fn run_inner_step_via_machine<H: V3TurnHost>(
    host: &mut H,
    turn: &mut TurnContext,
    client: &dyn LlmClient,
    mode: TurnLoopMode,
    machine: &LiveTurnMachine,
    tool_catalog: &mut [Tool],
    active_tool_names: &mut HashSet<String>,
    force_update_plan_first: bool,
    loop_state: &mut LiveOuterLoopState,
    loop_guard: &mut LoopGuard,
    tool_registry: Option<&H::ToolRegistry>,
) -> V3StepOutcome {
    if host.kernel_machine_mode().uses_v3_turn_loop() {
        let model = host.session_mut().model.clone();
        let token_budget = context_input_budget(&model, TURN_MAX_OUTPUT_TOKENS)
            .map(|b| b.min(u32::MAX as usize) as u32)
            .unwrap_or(TURN_MAX_OUTPUT_TOKENS);
        let projection = TurnKernelProjection::from_events(&host.kernel_turn_events());
        let plan = machine.inner_step_baseline_plan(&projection, token_budget);
        super::v3_driver::log_inner_step_effect_plan(&turn.id, turn.step, &plan);
    }

    let outcome = run_v3_turn_step_unified(
        host,
        turn,
        client,
        mode,
        tool_catalog,
        active_tool_names,
        force_update_plan_first,
        &mut loop_state.stream_retry_attempts,
        &mut loop_state.context_recovery_attempts,
        &mut loop_state.length_continuations,
        &mut loop_state.turn_error,
        loop_guard,
        loop_state.consecutive_tool_error_steps,
        tool_registry,
    )
    .await;

    if host.kernel_machine_mode().uses_v3_turn_loop() {
        let turn_events = host.kernel_turn_events();
        if model_request_issued_for_step(&turn_events, turn.step).is_some() {
            if let Some(warn) = machine.verify_inner_step_from_turn_log(&turn_events, turn.step) {
                tracing::warn!(
                    target: "kernel_v3",
                    turn_id = %turn.id,
                    step = turn.step,
                    diff = %warn,
                    "inner step ModelRequestIssued replay coherence diff (log-driven)"
                );
            } else {
                super::v3_driver::log_inner_step_model_request_replay_ok(&turn.id, turn.step);
            }
            if let Some(warn) =
                machine.verify_inner_step_slice_from_turn_log(&turn_events, turn.step)
            {
                tracing::warn!(
                    target: "kernel_v3",
                    turn_id = %turn.id,
                    step = turn.step,
                    diff = %warn,
                    "inner step slice replay coherence diff (log-driven)"
                );
            } else {
                super::v3_driver::log_inner_step_slice_replay_ok(&turn.id, turn.step);
            }
        }
    }

    outcome
}

pub async fn refresh_system_prompt_via_machine<H: OuterLoopHost>(
    host: &mut H,
    turn: &TurnContext,
    mode: TurnLoopMode,
    machine: &LiveTurnMachine,
) {
    if host.kernel_machine_mode().uses_v3_turn_loop() {
        let refresh = machine
            .outer_pre_inner_effect_plan()
            .system_prompt_refresh
            .clone();
        super::v3_driver::log_system_prompt_refresh_plan(&turn.id, turn.step, &refresh);
        if host.try_run_system_prompt_refresh(turn, mode).await {
            super::v3_driver::log_system_prompt_refresh_via_runtime_ops(&turn.id, turn.step);
            return;
        }
    }
    host.refresh_system_prompt(mode).await;
}

pub async fn run_outer_pre_inner_step_via_machine<H: OuterLoopHost>(
    host: &mut H,
    turn: &mut TurnContext,
    client: &dyn LlmClient,
    mode: TurnLoopMode,
    loop_state: &mut LiveOuterLoopState,
    machine: &LiveTurnMachine,
    step_budget_increment: u32,
) -> OuterPreInnerStepOutcome {
    drain_live_steers_via_machine(host, turn, machine).await;

    refresh_system_prompt_via_machine(host, turn, mode, machine).await;
    host.maybe_lht_pre_request_hooks(mode).await;

    if turn.at_max_steps() {
        let boundary_counters = loop_state.boundary_counters();
        match machine.pre_inner_step_limit_gate(mode, boundary_counters, true) {
            PreInnerStepLimitGate::NotAtCap => {}
            PreInnerStepLimitGate::AwaitHostContinue
                if host.maybe_continue_at_step_limit(turn).await =>
            {
                let grant = machine.step_limit_continuation_grant(
                    turn,
                    boundary_counters,
                    step_budget_increment,
                );
                apply_outer_boundary_grant(host, turn, &grant, loop_state, machine).await;
                return OuterPreInnerStepOutcome::ContinueOuterLoop;
            }
            PreInnerStepLimitGate::AwaitHostContinue | PreInnerStepLimitGate::TerminateAtCap => {
                let _ = host
                    .tx_event()
                    .send(Event::status("Reached maximum steps"))
                    .await;
                return OuterPreInnerStepOutcome::BreakOuterLoop;
            }
        }
    }

    if host.kernel_machine_mode().uses_v3_turn_loop() {
        super::v3_driver::log_v3_pre_inner_step_plan(host, &turn.id, turn.step);
        super::v3_driver::log_outer_pre_inner_effect_plan(
            &turn.id,
            turn.step,
            &machine.outer_pre_inner_effect_plan(),
        );
    }

    run_pre_inner_step_baseline_via_machine(host, client, turn, machine).await;

    if run_capacity_pre_request_hold_via_machine(host, turn, Some(client), mode, machine).await {
        return OuterPreInnerStepOutcome::ContinueOuterLoop;
    }

    let model = host.session_mut().model.clone();
    if let Some(input_budget) = context_input_budget(&model, TURN_MAX_OUTPUT_TOKENS) {
        let estimated_input = host.estimated_input_tokens();
        let overflow_counters = loop_state.boundary_counters();
        match machine.pre_inner_overflow_gate(
            mode,
            overflow_counters,
            loop_state.context_recovery_attempts,
            estimated_input,
            input_budget,
        ) {
            PreInnerOverflowGate::WithinBudget => {}
            PreInnerOverflowGate::AwaitCycleHandoff
                if host
                    .maybe_cycle_handoff_on_context_overflow(turn, mode)
                    .await =>
            {
                let input_budget_u32 = input_budget.min(u32::MAX as usize) as u32;
                let grant =
                    machine.overflow_cycle_handoff_grant(turn, overflow_counters, input_budget_u32);
                apply_outer_boundary_grant(host, turn, &grant, loop_state, machine).await;
                return OuterPreInnerStepOutcome::ContinueOuterLoop;
            }
            PreInnerOverflowGate::HardFail | PreInnerOverflowGate::AwaitCycleHandoff => {
                let message = machine.overflow_hard_fail_message(estimated_input, input_budget);
                loop_state.turn_error = Some(message.clone());
                let _ = host
                    .tx_event()
                    .send(Event::error(ErrorEnvelope::context_overflow(message)))
                    .await;
                return OuterPreInnerStepOutcome::Failed;
            }
            PreInnerOverflowGate::AwaitBudgetRecompile
                if host
                    .recover_context_overflow(
                        client,
                        "preflight token budget",
                        TURN_MAX_OUTPUT_TOKENS,
                    )
                    .await =>
            {
                let input_budget_u32 = input_budget.min(u32::MAX as usize) as u32;
                let grant = machine.overflow_budget_recompile_grant(
                    turn,
                    overflow_counters,
                    loop_state.context_recovery_attempts,
                    input_budget_u32,
                );
                apply_outer_boundary_grant(host, turn, &grant, loop_state, machine).await;
                return OuterPreInnerStepOutcome::ContinueOuterLoop;
            }
            PreInnerOverflowGate::AwaitBudgetRecompile => {}
        }
    }

    OuterPreInnerStepOutcome::ProceedToInnerStep
}

#[allow(clippy::too_many_arguments)]
pub async fn run_outer_post_inner_step_via_machine<H: OuterLoopHost>(
    host: &mut H,
    turn: &mut TurnContext,
    mode: TurnLoopMode,
    loop_state: &mut LiveOuterLoopState,
    machine: &LiveTurnMachine,
    phase: &TurnLoopToolPhaseOutcome,
    pending_steers: &mut Vec<String>,
    loop_guard: &mut LoopGuard,
) -> OuterPostInnerStepOutcome {
    if host.kernel_machine_mode().uses_v3_turn_loop() {
        super::v3_driver::log_outer_post_inner_effect_plan(
            &turn.id,
            turn.step,
            &machine.outer_post_inner_effect_plan(),
        );
    }

    if phase.break_outer_loop {
        let boundary_counters = loop_state.boundary_counters();
        match machine.post_inner_loop_guard_gate(mode, phase.loop_guard_halted, boundary_counters) {
            PostInnerLoopGuardGate::NotHalted => return OuterPostInnerStepOutcome::BreakOuterLoop,
            PostInnerLoopGuardGate::AwaitHostContinue
                if host.maybe_continue_after_loop_guard_halt(turn).await =>
            {
                let grant = machine.loop_guard_continuation_grant(turn, boundary_counters);
                apply_outer_boundary_grant(host, turn, &grant, loop_state, machine).await;
                loop_guard.reset_failures();
                turn.next_step();
                return OuterPostInnerStepOutcome::ContinueOuterLoop;
            }
            PostInnerLoopGuardGate::AwaitHostContinue | PostInnerLoopGuardGate::Terminate => {
                return OuterPostInnerStepOutcome::BreakOuterLoop;
            }
        }
    }

    if phase.continue_outer_loop {
        if phase.step_error_count > 0 {
            loop_state.consecutive_tool_error_steps =
                loop_state.consecutive_tool_error_steps.saturating_add(1);
        } else {
            loop_state.consecutive_tool_error_steps = 0;
        }
        turn.next_step();
        return OuterPostInnerStepOutcome::ContinueOuterLoop;
    }

    if !pending_steers.is_empty() {
        let workspace = host.workspace().to_path_buf();
        for steer in pending_steers.drain(..) {
            host.session_mut()
                .working_set
                .observe_user_message(&steer, &workspace);
            host.add_session_message(Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: steer,
                    cache_control: None,
                }],
            })
            .await;
        }
    }

    if phase.step_error_count > 0 {
        loop_state.consecutive_tool_error_steps =
            loop_state.consecutive_tool_error_steps.saturating_add(1);
    } else {
        loop_state.consecutive_tool_error_steps = 0;
    }

    if run_capacity_error_escalation_hold_via_machine(
        host,
        turn,
        mode,
        phase.step_error_count,
        loop_state,
        &phase.step_error_categories,
        machine,
    )
    .await
    {
        turn.next_step();
        return OuterPostInnerStepOutcome::ContinueOuterLoop;
    }

    host.maybe_inject_scratchpad_reminder(turn).await;

    let cycle_counters = loop_state.boundary_counters();
    if matches!(
        machine.post_inner_cycle_advance_gate(mode, cycle_counters),
        PostInnerCycleAdvanceGate::AwaitHostAdvance
    ) && host.maybe_advance_cycle_at_checkpoint(mode, turn).await
    {
        let grant = machine.in_turn_cycle_advance_grant(cycle_counters);
        apply_outer_boundary_grant(host, turn, &grant, loop_state, machine).await;
        turn.next_step();
        return OuterPostInnerStepOutcome::ContinueOuterLoop;
    }

    OuterPostInnerStepOutcome::AdvanceStep
}

pub async fn apply_outer_boundary_grant<H: OuterLoopHost>(
    host: &mut H,
    turn: &mut TurnContext,
    grant: &OuterBoundaryGrant,
    loop_state: &mut LiveOuterLoopState,
    machine: &LiveTurnMachine,
) {
    if host.kernel_machine_mode().uses_v3_turn_loop() {
        if let Some(kind) = grant.boundary_kind {
            tracing::debug!(
                target: "kernel_v3",
                turn_id = %turn.id,
                step = turn.step,
                boundary = ?kind,
                replay_effects = machine.boundary_confirmation_effects(kind).len(),
                "outer boundary grant replay plan"
            );
        }
        if let Some(summary) = machine.verify_boundary_grant(grant) {
            tracing::warn!(
                target: "kernel_v3",
                turn_id = %turn.id,
                step = turn.step,
                summary,
                "outer boundary grant replay coherence diff"
            );
        }
    }

    loop_state.apply_grant(grant, turn);
    if let Some(status) = &grant.status {
        let _ = host
            .tx_event()
            .send(Event::status(status.message.clone()))
            .await;
    }
    if let Some(event) = &grant.kernel_event {
        emit_kernel_event(host, event.clone());
    }
    if let Some(kind) = grant.boundary_kind {
        super::v3_driver::log_v3_outer_boundary(
            host,
            kind,
            &turn.id,
            turn.step,
            outer_boundary_grant_log_count(grant),
        );
    }
}

pub async fn run_pre_inner_step_baseline_via_machine<H: OuterLoopHost>(
    host: &mut H,
    client: &dyn LlmClient,
    turn: &TurnContext,
    _machine: &LiveTurnMachine,
) {
    run_pre_inner_step_baseline(host, client, turn).await;
}

pub async fn run_capacity_pre_request_hold_via_machine<H: OuterLoopHost>(
    host: &mut H,
    turn: &TurnContext,
    client: Option<&dyn LlmClient>,
    mode: TurnLoopMode,
    _machine: &LiveTurnMachine,
) -> bool {
    run_capacity_pre_request_hold(host, turn, client, mode).await
}

pub async fn run_capacity_error_escalation_hold_via_machine<H: OuterLoopHost>(
    host: &mut H,
    turn: &mut TurnContext,
    mode: TurnLoopMode,
    step_error_count: usize,
    loop_state: &LiveOuterLoopState,
    error_categories: &[ErrorCategory],
    _machine: &LiveTurnMachine,
) -> bool {
    if matches!(
        post_inner_error_escalation_gate(
            step_error_count,
            loop_state.consecutive_tool_error_steps,
            error_categories,
        ),
        PostInnerErrorEscalationGate::Skip
    ) {
        return false;
    }
    run_capacity_error_escalation_hold(
        host,
        turn,
        mode,
        step_error_count,
        loop_state.consecutive_tool_error_steps,
        error_categories,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_machine_replay_delegates_to_replay_turn_machine() {
        let mut machine = LiveTurnMachine::default();
        let projection = TurnKernelProjection::default();
        let out = machine.step(
            &projection,
            KernelEvent::TurnStarted {
                turn_id: "t".into(),
                mode: TurnLoopMode::Agent,
                input_text: String::new(),
                max_steps: 8,
            },
        );
        assert!(out.halt.is_none());
    }

    #[test]
    fn pre_inner_baseline_effect_plan_length() {
        let machine = LiveTurnMachine::default();
        assert_eq!(machine.pre_inner_baseline_effects().len(), 2);
    }

    #[test]
    fn outer_pre_inner_effect_plan_includes_host_refresh_seam() {
        let machine = LiveTurnMachine::default();
        let plan = machine.outer_pre_inner_effect_plan();
        assert!(plan.live_steer_inject_per_message);
        assert_eq!(plan.system_prompt_refresh.effects.len(), 3);
        assert!(!plan.system_prompt_refresh.host_io_required);
        assert_eq!(plan.baseline.len(), 2);
    }

    #[test]
    fn inner_step_live_plan_documents_dynamic_tail() {
        let machine = LiveTurnMachine::default();
        let plan = machine.inner_step_live_plan(&TurnKernelProjection::default(), 8192, None);
        assert!(plan.baseline.execute_batch_per_call);
        assert!(plan.baseline.notify_lsp_tail);
    }

    #[test]
    fn verify_inner_step_from_turn_log_accepts_baseline_fixture() {
        use crate::engine::request_fingerprint::RequestFingerprint;
        use crate::turn::TurnLoopMode;

        let machine = LiveTurnMachine::default();
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t".into(),
                mode: TurnLoopMode::Agent,
                input_text: String::new(),
                max_steps: 8,
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t".into(),
                step_idx: 1,
                request_fp: RequestFingerprint {
                    static_prefix_sha256: String::new(),
                    full_prefix_sha256: String::new(),
                },
                token_budget: 8192,
            },
        ];
        assert!(
            machine
                .verify_inner_step_from_turn_log(&events, 1)
                .is_none()
        );
    }

    #[test]
    fn outer_step_frame_effect_plan_documents_reset_sync_cancel() {
        let machine = LiveTurnMachine::default();
        let plan = machine.outer_step_frame_effect_plan();
        assert!(plan.scratchpad_step_reset);
        assert!(plan.kernel_turn_frame_sync);
        assert!(plan.cancel_check);
    }

    #[test]
    fn outer_post_inner_effect_plan_documents_conditional_slots() {
        let machine = LiveTurnMachine::default();
        let plan = machine.outer_post_inner_effect_plan();
        assert!(plan.error_escalation_capacity_hold);
        assert!(plan.loop_guard_continuation.is_some());
        assert!(plan.in_turn_cycle_advance.is_some());
    }

    #[test]
    fn boundary_confirmation_step_limit_is_inject_steer() {
        let machine = LiveTurnMachine::default();
        let effects = machine.boundary_confirmation_effects(OuterBoundaryKind::StepLimit);
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], Effect::InjectSteer { .. }));
    }

    #[test]
    fn loop_state_boundary_counters_roundtrip() {
        let state = LiveOuterLoopState {
            step_limit_continuations: 2,
            in_turn_cycle_advances: 1,
            ..Default::default()
        };
        let counters = state.boundary_counters();
        assert_eq!(counters.step_limit_continuations, 2);
        assert_eq!(counters.in_turn_cycle_advances, 1);
    }

    #[test]
    fn verify_boundary_grant_accepts_step_limit_grant() {
        use crate::engine::turn_loop::continuation_boundary_policy::OuterBoundaryCounters;
        use crate::engine::turn_loop::live_turn_outer_driver::apply_step_limit_continuation;

        let machine = LiveTurnMachine::default();
        let mut turn = TurnContext::new(8);
        turn.id = "t1".into();
        let grant = apply_step_limit_continuation(&turn, OuterBoundaryCounters::default(), 8);
        assert!(machine.verify_boundary_grant(&grant).is_none());
    }
}
