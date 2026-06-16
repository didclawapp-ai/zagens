//! Effect interpreter — Phase 3b batch 2/3/6f.
//!
//! When `[kernel] machine = "v3"`, turn-step IO is routed through this module
//! instead of direct `run_streaming_phase` / `run_tool_execution_phase` calls.

use std::collections::HashSet;

use zagens_core::chat::{LlmClient, Tool};
use zagens_core::engine::context::{TURN_MAX_OUTPUT_TOKENS, context_input_budget};
use zagens_core::engine::loop_guard::LoopGuard;
use zagens_core::engine::streaming::ToolUseState;
use zagens_core::engine::turn_loop::control::{
    TurnLoopStreamingPhaseOutcome, TurnLoopToolPhaseOutcome,
};
use zagens_core::engine::turn_loop::live_turn_machine::LiveTurnMachine;
use zagens_core::engine::turn_loop::v3_step::{V3StepOutcome, execute_batch_call_ids};
use zagens_core::engine::turn_loop::{
    InnerStepHost, TurnLoopSessionHost, V3TurnHost, streaming_phase, tool_phase,
};
use zagens_core::engine::turn_machine::{Effect, TurnKernelProjection, events_for_step};
use zagens_core::turn::{TurnContext, TurnLoopMode};

use super::Engine;

/// Result of interpreting one [`Effect`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterpretOutcome {
    /// Effect ran through the turn-loop implementation.
    Executed,
    /// Effect handled by a stub / no-op delegate.
    DelegatedLegacy,
    /// Effect kind not yet implemented in the interpreter.
    NotImplemented,
}

/// Mutable v3 step IO context while interpreting a [`plan_v3_step_effects`] chain.
struct V3StepInterpretContext<'a> {
    turn: &'a mut TurnContext,
    client: &'a dyn LlmClient,
    mode: TurnLoopMode,
    tool_catalog: &'a mut [Tool],
    active_tool_names: &'a mut HashSet<String>,
    force_update_plan_first: bool,
    stream_retry_attempts: &'a mut u32,
    context_recovery_attempts: &'a mut u8,
    length_continuations: &'a mut u32,
    turn_error: &'a mut Option<String>,
    loop_guard: &'a mut LoopGuard,
    consecutive_tool_error_steps: u32,
    tool_registry: Option<&'a <Engine as InnerStepHost>::ToolRegistry>,
    stream: Option<TurnLoopStreamingPhaseOutcome>,
    tools: Option<TurnLoopToolPhaseOutcome>,
    execute_batch_ran: bool,
}

/// Executes kernel [`Effect`] values against the runtime engine.
pub struct EffectInterpreter<'a> {
    engine: &'a mut Engine,
}

impl<'a> EffectInterpreter<'a> {
    pub fn new(engine: &'a mut Engine) -> Self {
        Self { engine }
    }

    /// Run one v3 turn step: `CallModel` then optional `ExecuteBatch` effects.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_v3_turn_step(
        &mut self,
        turn: &mut TurnContext,
        client: &dyn LlmClient,
        mode: TurnLoopMode,
        tool_catalog: &mut [Tool],
        active_tool_names: &mut HashSet<String>,
        force_update_plan_first: bool,
        stream_retry_attempts: &mut u32,
        context_recovery_attempts: &mut u8,
        length_continuations: &mut u32,
        turn_error: &mut Option<String>,
        loop_guard: &mut LoopGuard,
        consecutive_tool_error_steps: u32,
        tool_registry: Option<&<Engine as InnerStepHost>::ToolRegistry>,
    ) -> V3StepOutcome
    where
        Engine: V3TurnHost,
    {
        self.engine.clear_v3_approval_outcomes();
        super::memory_plane_compiler_ops::clear_memory_query_compiler_sources(
            self.engine.runtime_ext_mut(),
        );
        let model = self.engine.session_mut().model.clone();
        let token_budget = context_input_budget(&model, TURN_MAX_OUTPUT_TOKENS)
            .map(|b| b.min(u32::MAX as usize) as u32)
            .unwrap_or(TURN_MAX_OUTPUT_TOKENS);

        let mut ctx = V3StepInterpretContext {
            turn,
            client,
            mode,
            tool_catalog,
            active_tool_names,
            force_update_plan_first,
            stream_retry_attempts,
            context_recovery_attempts,
            length_continuations,
            turn_error,
            loop_guard,
            consecutive_tool_error_steps,
            tool_registry,
            stream: None,
            tools: None,
            execute_batch_ran: false,
        };

        let turn_events = self
            .engine
            .runtime_ext()
            .kernel_turn_events
            .turn_events()
            .to_vec();
        let projection = TurnKernelProjection::from_events(&turn_events);
        let episodic_hints =
            zagens_core::engine::turn_loop::memory_plane_episodic_policy::MemoryPlaneEpisodicHints {
                topic_memory_enabled: self
                    .engine
                    .runtime_ext()
                    .config_ext
                    .topic_memory
                    .enabled,
            };
        let machine = LiveTurnMachine::default();
        let inner_plan =
            machine.inner_step_live_plan(&projection, token_budget, Some(episodic_hints));
        let pre_call = inner_plan.baseline.pre_call_model.clone();
        if !pre_call.is_empty() {
            tracing::info!(
                target: "kernel_v3",
                turn_id = %ctx.turn.id,
                step = ctx.turn.step,
                query_count = pre_call.len(),
                "v3 step: QueryMemory (LiveTurnMachine pre-CallModel plan)"
            );
            for effect in pre_call {
                let outcome = self.interpret_v3_step_effect(effect, &mut ctx).await;
                debug_assert_eq!(
                    outcome,
                    InterpretOutcome::Executed,
                    "QueryMemory must execute"
                );
            }
        }

        let call_outcomes = self
            .interpret_all(vec![inner_plan.baseline.call_model.clone()], Some(&mut ctx))
            .await;
        debug_assert_eq!(
            call_outcomes.first(),
            Some(&InterpretOutcome::Executed),
            "CallModel must execute"
        );

        let stream_ref = ctx
            .stream
            .as_ref()
            .expect("CallModel effect must populate stream outcome");
        let call_ids = execute_batch_call_ids(&stream_ref.tool_uses);
        zagens_core::engine::turn_loop::v3_driver::log_v3_step_effect_plan(
            &ctx.turn.id,
            ctx.turn.step,
            token_budget,
            &call_ids,
        );

        let tool_registry = ctx.tool_registry;
        let approval_plan: Vec<(String, String)> = stream_ref
            .tool_uses
            .iter()
            .filter_map(|tool_use| {
                let meta = self.engine.tool_plan_approval_meta(
                    &tool_use.name,
                    &tool_use.input,
                    tool_registry,
                );
                if !meta.approval_required
                    || self
                        .engine
                        .approval_cache_hit(&tool_use.name, &tool_use.input)
                {
                    return None;
                }
                Some((tool_use.id.clone(), meta.approval_description))
            })
            .collect();
        for (call_id, description) in approval_plan {
            tracing::info!(
                target: "kernel_v3",
                turn_id = %ctx.turn.id,
                step = ctx.turn.step,
                call_id = %call_id,
                "v3 step: RequestApproval (pre-ExecuteBatch)"
            );
            let outcome = self
                .interpret_v3_step_effect(
                    Effect::RequestApproval {
                        call_id,
                        description,
                    },
                    &mut ctx,
                )
                .await;
            debug_assert_eq!(
                outcome,
                InterpretOutcome::Executed,
                "RequestApproval must execute"
            );
        }

        let plan = machine.inner_step_execute_batch_effects(token_budget, &call_ids);
        if !plan.is_empty() {
            let execute_outcomes = self.interpret_all(plan, Some(&mut ctx)).await;
            if !call_ids.is_empty() {
                debug_assert!(
                    execute_outcomes
                        .iter()
                        .all(|o| *o == InterpretOutcome::Executed),
                    "ExecuteBatch plan tail must execute"
                );
            }
        }

        if ctx.execute_batch_ran {
            let turn_events = self.engine.runtime_ext().kernel_turn_events.turn_events();
            let step_events = events_for_step(turn_events, ctx.turn.step);
            let notify_tail = machine.inner_step_notify_lsp_effects(&step_events);
            if !notify_tail.is_empty() {
                tracing::info!(
                    target: "kernel_v3",
                    turn_id = %ctx.turn.id,
                    step = ctx.turn.step,
                    notify_count = notify_tail.len(),
                    "v3 step: NotifyLsp tail (LiveTurnMachine plan)"
                );
                let notify_outcomes = self.interpret_all(notify_tail, Some(&mut ctx)).await;
                debug_assert!(
                    notify_outcomes
                        .iter()
                        .all(|o| *o == InterpretOutcome::Executed),
                    "NotifyLsp tail must execute"
                );
            }
        }

        let stream = ctx
            .stream
            .take()
            .expect("CallModel effect must populate stream outcome");
        let tools = ctx.tools.take().unwrap_or_default();
        V3StepOutcome { stream, tools }
    }

    /// Interpret one v3 step effect against live turn-loop IO (`CallModel` / `ExecuteBatch`).
    async fn interpret_v3_step_effect(
        &mut self,
        effect: Effect,
        ctx: &mut V3StepInterpretContext<'_>,
    ) -> InterpretOutcome
    where
        Engine: V3TurnHost,
    {
        match effect {
            Effect::CallModel { token_budget } => {
                tracing::info!(
                    target: "kernel_v3",
                    turn_id = %ctx.turn.id,
                    step = ctx.turn.step,
                    token_budget,
                    "v3 step: CallModel (effect plan)"
                );
                let stream = self
                    .run_call_model_step(
                        ctx.turn,
                        ctx.client,
                        ctx.mode,
                        ctx.tool_catalog,
                        ctx.active_tool_names,
                        ctx.force_update_plan_first,
                        ctx.stream_retry_attempts,
                        ctx.context_recovery_attempts,
                        ctx.length_continuations,
                        ctx.turn_error,
                        token_budget,
                    )
                    .await;
                ctx.stream = Some(stream);
                InterpretOutcome::Executed
            }
            Effect::ExecuteBatch { call_ids } => {
                if ctx.execute_batch_ran {
                    return InterpretOutcome::Executed;
                }
                let Some(stream) = ctx.stream.as_mut() else {
                    return InterpretOutcome::NotImplemented;
                };
                if stream.tool_uses.is_empty() {
                    return InterpretOutcome::Executed;
                }
                tracing::info!(
                    target: "kernel_v3",
                    turn_id = %ctx.turn.id,
                    step = ctx.turn.step,
                    call_count = call_ids.len(),
                    "v3 step: ExecuteBatch (effect plan)"
                );
                let tools = self
                    .run_execute_batch_step(
                        ctx.turn,
                        ctx.mode,
                        &mut stream.tool_uses,
                        ctx.tool_catalog,
                        ctx.active_tool_names,
                        ctx.loop_guard,
                        ctx.consecutive_tool_error_steps,
                        ctx.tool_registry,
                        call_ids,
                    )
                    .await;
                ctx.tools = Some(tools);
                ctx.execute_batch_ran = true;
                InterpretOutcome::Executed
            }
            Effect::InjectSteer { text } => {
                self.engine
                    .run_inject_steer_effect(&ctx.turn.id, ctx.turn.step, text)
                    .await;
                InterpretOutcome::Executed
            }
            Effect::RequestApproval {
                call_id,
                description,
            } => {
                let Some(stream) = ctx.stream.as_ref() else {
                    return InterpretOutcome::NotImplemented;
                };
                let Some(tool_use) = stream.tool_uses.iter().find(|t| t.id == call_id) else {
                    return InterpretOutcome::NotImplemented;
                };
                let _ = self
                    .engine
                    .run_request_approval_effect(
                        &ctx.turn.id,
                        &call_id,
                        &tool_use.name,
                        &tool_use.input,
                        &description,
                        tool_use.caller.as_ref(),
                    )
                    .await;
                InterpretOutcome::Executed
            }
            Effect::NotifyLsp { tool_name } => {
                let _ = tool_name;
                self.engine.flush_pending_lsp_diagnostics().await;
                InterpretOutcome::Executed
            }
            Effect::QueryMemory { layer, query_key } => {
                self.engine.run_query_memory_effect(layer, &query_key).await;
                InterpretOutcome::Executed
            }
            _ => self.interpret(effect).await,
        }
    }

    /// Run the streaming phase as a `CallModel` effect.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_call_model_step(
        &mut self,
        turn: &mut TurnContext,
        client: &dyn LlmClient,
        mode: TurnLoopMode,
        tool_catalog: &[Tool],
        active_tool_names: &HashSet<String>,
        force_update_plan_first: bool,
        stream_retry_attempts: &mut u32,
        context_recovery_attempts: &mut u8,
        length_continuations: &mut u32,
        turn_error: &mut Option<String>,
        token_budget: u32,
    ) -> TurnLoopStreamingPhaseOutcome
    where
        Engine: V3TurnHost,
    {
        let _ = token_budget;
        streaming_phase::run_streaming_phase(
            self.engine,
            turn,
            client,
            mode,
            tool_catalog,
            active_tool_names,
            force_update_plan_first,
            stream_retry_attempts,
            context_recovery_attempts,
            length_continuations,
            turn_error,
        )
        .await
    }

    /// Run the tool phase as an `ExecuteBatch` effect.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_execute_batch_step(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        tool_uses: &mut [ToolUseState],
        tool_catalog: &mut [Tool],
        active_tool_names: &mut HashSet<String>,
        loop_guard: &mut LoopGuard,
        consecutive_tool_error_steps: u32,
        tool_registry: Option<&<Engine as InnerStepHost>::ToolRegistry>,
        call_ids: Vec<String>,
    ) -> TurnLoopToolPhaseOutcome
    where
        Engine: V3TurnHost,
    {
        let _ = call_ids;
        tool_phase::run_tool_execution_phase(
            self.engine,
            turn,
            mode,
            tool_uses,
            tool_catalog,
            active_tool_names,
            loop_guard,
            consecutive_tool_error_steps,
            tool_registry,
        )
        .await
    }

    /// Interpret one effect. Standalone API for stateless effects (compaction, steer stub).
    pub async fn interpret(&mut self, effect: Effect) -> InterpretOutcome {
        match effect {
            Effect::CallModel { .. } | Effect::ExecuteBatch { .. } => {
                InterpretOutcome::NotImplemented
            }
            Effect::RequestApproval { .. } => {
                if self.engine.effect_replay_anchor_only() {
                    InterpretOutcome::Executed
                } else {
                    InterpretOutcome::NotImplemented
                }
            }
            Effect::InjectSteer { text } => {
                let ext = self.engine.runtime_ext();
                let turn_id = ext
                    .kernel_active_turn_id
                    .clone()
                    .unwrap_or_else(|| "effect-interpreter".to_string());
                let step_idx = ext.kernel_active_step;
                self.engine
                    .run_inject_steer_effect(&turn_id, step_idx, text)
                    .await;
                InterpretOutcome::Executed
            }
            Effect::RunCompaction => {
                self.engine.run_compaction_effect().await;
                InterpretOutcome::Executed
            }
            Effect::NotifyLsp { tool_name } => {
                let _ = tool_name;
                self.engine.flush_pending_lsp_diagnostics().await;
                InterpretOutcome::Executed
            }
            Effect::Sleep { millis } => {
                self.engine.run_sleep_effect(millis).await;
                InterpretOutcome::Executed
            }
            Effect::QueryMemory { layer, query_key } => {
                self.engine.run_query_memory_effect(layer, &query_key).await;
                InterpretOutcome::Executed
            }
            Effect::RunLayeredContextCheckpoint => {
                self.engine.run_layered_context_checkpoint_effect().await;
                InterpretOutcome::Executed
            }
            Effect::RefreshSystemPrompt => InterpretOutcome::NotImplemented,
            _ => InterpretOutcome::NotImplemented,
        }
    }

    /// Interpret effects in order. When `v3_ctx` is set, routes `CallModel` /
    /// `ExecuteBatch` through the v3 step IO path; otherwise uses stateless `interpret`.
    pub async fn interpret_all(
        &mut self,
        effects: Vec<Effect>,
        mut v3_ctx: Option<&mut V3StepInterpretContext<'_>>,
    ) -> Vec<InterpretOutcome> {
        let mut out = Vec::with_capacity(effects.len());
        for effect in effects {
            let outcome = match v3_ctx {
                Some(ref mut ctx) => self.interpret_v3_step_effect(effect, ctx).await,
                None => self.interpret(effect).await,
            };
            out.push(outcome);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::engine::turn_machine::{Effect, plan_v3_step_effects};

    #[test]
    fn interpret_outcome_variants_distinct() {
        assert_ne!(InterpretOutcome::Executed, InterpretOutcome::NotImplemented);
        assert_ne!(
            InterpretOutcome::DelegatedLegacy,
            InterpretOutcome::NotImplemented
        );
    }

    #[test]
    fn plan_v3_step_effects_matches_call_and_batch_shape() {
        let effects = plan_v3_step_effects(8192, &["c1".into(), "c2".into()]);
        assert_eq!(effects.len(), 3);
        assert!(matches!(effects[0], Effect::CallModel { .. }));
        assert!(matches!(effects[1], Effect::ExecuteBatch { .. }));
        assert!(matches!(effects[2], Effect::ExecuteBatch { .. }));
    }

    #[test]
    fn v3_step_plan_execute_tail_matches_call_ids() {
        let call_ids = ["c1".to_string(), "c2".to_string()];
        let plan = plan_v3_step_effects(8192, &call_ids);
        let execute_tail: Vec<_> = plan.into_iter().skip(1).collect();
        assert_eq!(execute_tail.len(), call_ids.len());
        for (effect, call_id) in execute_tail.iter().zip(call_ids.iter()) {
            match effect {
                Effect::ExecuteBatch { call_ids: ids } => {
                    assert_eq!(ids, std::slice::from_ref(call_id));
                }
                other => panic!("expected ExecuteBatch, got {other:?}"),
            }
        }
    }
}
