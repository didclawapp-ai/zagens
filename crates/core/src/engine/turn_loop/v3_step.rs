//! Phase 3b batch 2 — v3 turn step driver (CallModel → ExecuteBatch effects).

use std::collections::HashSet;

use crate::chat::{LlmClient, Tool};
use crate::engine::context::{TURN_MAX_OUTPUT_TOKENS, context_input_budget};
use crate::engine::loop_guard::LoopGuard;
use crate::engine::streaming::ToolUseState;
use crate::turn::{TurnContext, TurnLoopMode};

use super::control::{TurnLoopStreamingPhaseOutcome, TurnLoopToolPhaseOutcome};
use super::host::TurnLoopHost;
use super::{streaming_phase, tool_phase};

/// Combined outcome of one v3 turn step (model stream + optional tool batch).
#[derive(Debug)]
pub struct V3StepOutcome {
    pub stream: TurnLoopStreamingPhaseOutcome,
    pub tools: TurnLoopToolPhaseOutcome,
}

#[must_use]
pub fn execute_batch_call_ids(tool_uses: &[ToolUseState]) -> Vec<String> {
    tool_uses.iter().map(|t| t.id.clone()).collect()
}

/// Core fallback: run CallModel then ExecuteBatch by calling streaming/tool phases directly.
#[allow(clippy::too_many_arguments)]
pub async fn run_v3_step<H: TurnLoopHost>(
    host: &mut H,
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
    tool_registry: Option<&H::ToolRegistry>,
) -> V3StepOutcome {
    let model = host.session_mut().model.clone();
    let token_budget = context_input_budget(&model, TURN_MAX_OUTPUT_TOKENS)
        .map(|b| b.min(u32::MAX as usize) as u32)
        .unwrap_or(TURN_MAX_OUTPUT_TOKENS);

    tracing::info!(
        target: "kernel_v3",
        turn_id = %turn.id,
        step = turn.step,
        token_budget,
        "v3 step: CallModel"
    );

    let mut stream = streaming_phase::run_streaming_phase(
        host,
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
    .await;

    let tools = if stream.tool_uses.is_empty() {
        TurnLoopToolPhaseOutcome::default()
    } else {
        let call_ids = execute_batch_call_ids(&stream.tool_uses);
        tracing::info!(
            target: "kernel_v3",
            turn_id = %turn.id,
            step = turn.step,
            call_count = call_ids.len(),
            "v3 step: ExecuteBatch"
        );
        let _ = call_ids;
        tool_phase::run_tool_execution_phase(
            host,
            turn,
            mode,
            &mut stream.tool_uses,
            tool_catalog,
            active_tool_names,
            loop_guard,
            consecutive_tool_error_steps,
            tool_registry,
        )
        .await
    };

    V3StepOutcome { stream, tools }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_batch_call_ids_preserves_order() {
        let uses = vec![
            ToolUseState {
                id: "a".into(),
                name: "read_file".into(),
                input: serde_json::json!({}),
                caller: None,
                input_buffer: String::new(),
            },
            ToolUseState {
                id: "b".into(),
                name: "list_dir".into(),
                input: serde_json::json!({}),
                caller: None,
                input_buffer: String::new(),
            },
        ];
        assert_eq!(
            execute_batch_call_ids(&uses),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
