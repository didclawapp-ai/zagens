//! v3 turn step — routes IO through [`EffectInterpreter`].

use std::collections::HashSet;

use zagens_core::chat::{LlmClient, Tool};
use zagens_core::engine::TurnLoopHost;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::loop_guard::LoopGuard;
use zagens_core::engine::turn_loop::v3_step::V3StepOutcome;
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
    let mut interpreter = EffectInterpreter::new(engine);
    let outcome = interpreter
        .run_v3_turn_step(
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
        )
        .await;

    let turn_events = engine
        .runtime_ext()
        .kernel_projection_shadow
        .turn_events()
        .to_vec();
    let executed_tool_count = turn_events
        .iter()
        .filter(|event| {
            matches!(
                event,
                KernelEvent::ToolCallPlanned { step_idx, .. } if *step_idx == turn.step
            )
        })
        .count() as u32;
    engine.runtime_ext().kernel_v3_effect_shadow.verify_step(
        &turn_events,
        turn.step,
        executed_tool_count,
    );

    outcome
}
