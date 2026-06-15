//! v3 turn step — routes IO through [`EffectInterpreter`].

use std::collections::HashSet;

use zagens_core::chat::{LlmClient, Tool};
use zagens_core::engine::TurnLoopHost;
use zagens_core::engine::loop_guard::LoopGuard;
use zagens_core::engine::turn_loop::v3_step::{V3StepOutcome, execute_batch_call_ids};
use zagens_core::turn::{TurnContext, TurnLoopMode};

use super::Engine;
use super::effect_interpreter::EffectInterpreter;
use crate::tools::ToolRegistry;

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_v3_turn_step(
    engine: &mut Engine,
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
    tool_registry: Option<&ToolRegistry>,
) -> V3StepOutcome {
    let model = engine.session_mut().model.clone();
    let token_budget = zagens_core::engine::context::context_input_budget(
        &model,
        zagens_core::engine::context::TURN_MAX_OUTPUT_TOKENS,
    )
    .map(|b| b.min(u32::MAX as usize) as u32)
    .unwrap_or(zagens_core::engine::context::TURN_MAX_OUTPUT_TOKENS);

    tracing::info!(
        target: "kernel_v3",
        turn_id = %turn.id,
        step = turn.step,
        token_budget,
        "v3 step: CallModel (effect interpreter)"
    );

    let mut interpreter = EffectInterpreter::new(engine);
    let mut stream = interpreter
        .run_call_model_step(
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
            token_budget,
        )
        .await;

    let tools = if stream.tool_uses.is_empty() {
        zagens_core::engine::turn_loop::control::TurnLoopToolPhaseOutcome::default()
    } else {
        let call_ids = execute_batch_call_ids(&stream.tool_uses);
        tracing::info!(
            target: "kernel_v3",
            turn_id = %turn.id,
            step = turn.step,
            call_count = call_ids.len(),
            "v3 step: ExecuteBatch (effect interpreter)"
        );
        interpreter
            .run_execute_batch_step(
                turn,
                mode,
                &mut stream.tool_uses,
                tool_catalog,
                active_tool_names,
                loop_guard,
                consecutive_tool_error_steps,
                tool_registry,
                call_ids,
            )
            .await
    };

    V3StepOutcome { stream, tools }
}
