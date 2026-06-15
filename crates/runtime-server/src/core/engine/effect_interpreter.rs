//! Effect interpreter — Phase 3b batch 2/3.
//!
//! When `[kernel] machine = "v3"`, turn-step IO is routed through this module
//! instead of direct `run_streaming_phase` / `run_tool_execution_phase` calls.

use std::collections::HashSet;

use zagens_core::chat::{LlmClient, Tool};
use zagens_core::engine::loop_guard::LoopGuard;
use zagens_core::engine::streaming::ToolUseState;
use zagens_core::engine::turn_loop::control::{
    TurnLoopStreamingPhaseOutcome, TurnLoopToolPhaseOutcome,
};
use zagens_core::engine::turn_loop::{TurnLoopHost, streaming_phase, tool_phase};
use zagens_core::engine::turn_machine::Effect;
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

/// Executes kernel [`Effect`] values against the runtime engine.
pub struct EffectInterpreter<'a> {
    engine: &'a mut Engine,
}

impl<'a> EffectInterpreter<'a> {
    pub fn new(engine: &'a mut Engine) -> Self {
        Self { engine }
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
        Engine: TurnLoopHost,
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
        tool_registry: Option<&<Engine as TurnLoopHost>::ToolRegistry>,
        call_ids: Vec<String>,
    ) -> TurnLoopToolPhaseOutcome
    where
        Engine: TurnLoopHost,
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

    /// Interpret one effect.  Standalone API for future full event-driven loop.
    pub async fn interpret(&mut self, effect: Effect) -> InterpretOutcome {
        match effect {
            Effect::CallModel { .. } | Effect::ExecuteBatch { .. } => {
                InterpretOutcome::NotImplemented
            }
            Effect::RequestApproval { .. } => InterpretOutcome::NotImplemented,
            Effect::InjectSteer { text } => {
                let _ = text;
                InterpretOutcome::DelegatedLegacy
            }
            Effect::RunCompaction => InterpretOutcome::NotImplemented,
            Effect::NotifyLsp { .. } => InterpretOutcome::NotImplemented,
            Effect::Sleep { .. } => InterpretOutcome::NotImplemented,
            _ => InterpretOutcome::NotImplemented,
        }
    }

    /// Interpret a batch of effects in order (future full v3 loop entry point).
    pub async fn interpret_all(&mut self, effects: Vec<Effect>) -> Vec<InterpretOutcome> {
        let mut out = Vec::with_capacity(effects.len());
        for effect in effects {
            out.push(self.interpret(effect).await);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpret_outcome_variants_distinct() {
        assert_ne!(InterpretOutcome::Executed, InterpretOutcome::NotImplemented);
        assert_ne!(
            InterpretOutcome::DelegatedLegacy,
            InterpretOutcome::NotImplemented
        );
    }
}
