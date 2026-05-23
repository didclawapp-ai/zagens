//! Outer turn-step loop for agent turns (P2 PR4 — generic over [`TurnLoopHost`]).

use std::collections::HashSet;

use tracing::Instrument;

use crate::chat::{ContentBlock, Message, Tool};
use crate::engine::context::{
    context_input_budget, summarize_text, MAX_CONTEXT_RECOVERY_ATTEMPTS, TURN_MAX_OUTPUT_TOKENS,
};
use crate::engine::loop_guard::LoopGuard;
use crate::error_taxonomy::ErrorEnvelope;
use crate::events::Event;
use crate::turn::{TurnContext, TurnLoopMode, TurnOutcomeStatus};

use super::host::TurnLoopHost;

/// Run the multi-step agent turn loop until completion, interruption, or failure.
pub async fn handle_deepseek_turn<H: TurnLoopHost>(
    host: &mut H,
    turn: &mut TurnContext,
    tool_registry: Option<&H::ToolRegistry>,
    tools: Option<Vec<Tool>>,
    mode: TurnLoopMode,
    force_update_plan_first: bool,
) -> (TurnOutcomeStatus, Option<String>) {
    tracing::info!(turn_id = %turn.id, "turn loop start");

    let Some(client) = host.llm_client() else {
        return (
            TurnOutcomeStatus::Failed,
            Some("DeepSeek client is not configured".to_string()),
        );
    };

    let mut consecutive_tool_error_steps = 0u32;
    let mut turn_error: Option<String> = None;
    let mut context_recovery_attempts = 0u8;
    let mut tool_catalog = tools.unwrap_or_default();
    if !tool_catalog.is_empty() {
        host.prepare_tool_catalog(&mut tool_catalog);
    }
    let mut active_tool_names = host.initial_active_tool_names(&tool_catalog);
    let mut loop_guard = LoopGuard::default();
    let mut stream_retry_attempts: u32 = 0;

    loop {
        tracing::debug!(turn_id = %turn.id, step = turn.step, "turn step");

        host.reset_scratchpad_step();

        if host.cancel_token().is_cancelled() {
            let _ = host
                .tx_event()
                .send(Event::status("Request cancelled"))
                .await;
            return (TurnOutcomeStatus::Interrupted, None);
        }

        while let Ok(steer) = host.rx_steer_mut().try_recv() {
            let steer = steer.trim().to_string();
            if steer.is_empty() {
                continue;
            }
            let workspace = host.workspace().to_path_buf();
            host.session_mut()
                .working_set
                .observe_user_message(&steer, &workspace);
            host.add_session_message(Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: steer.clone(),
                    cache_control: None,
                }],
            })
            .await;
            let _ = host
                .tx_event()
                .send(Event::status(format!(
                    "Steer input accepted: {}",
                    summarize_text(&steer, 120)
                )))
                .await;
        }

        host.refresh_system_prompt(mode).await;

        if turn.at_max_steps() {
            let _ = host
                .tx_event()
                .send(Event::status("Reached maximum steps"))
                .await;
            break;
        }

        host.run_auto_compaction(client.as_ref()).await;

        if host
            .run_capacity_pre_request_checkpoint(turn, Some(client.as_ref()), mode)
            .await
        {
            continue;
        }

        let model = host.session_mut().model.clone();
        if let Some(input_budget) = context_input_budget(&model, TURN_MAX_OUTPUT_TOKENS) {
            let estimated_input = host.estimated_input_tokens();
            if estimated_input > input_budget {
                if context_recovery_attempts >= MAX_CONTEXT_RECOVERY_ATTEMPTS {
                    let message = format!(
                        "Context remains above model limit after {} recovery attempts \
                         (~{} token estimate, ~{} budget). Please run /compact or /clear.",
                        MAX_CONTEXT_RECOVERY_ATTEMPTS, estimated_input, input_budget
                    );
                    turn_error = Some(message.clone());
                    let _ = host
                        .tx_event()
                        .send(Event::error(ErrorEnvelope::context_overflow(message)))
                        .await;
                    return (TurnOutcomeStatus::Failed, turn_error);
                }

                if host
                    .recover_context_overflow(
                        client.as_ref(),
                        "preflight token budget",
                        TURN_MAX_OUTPUT_TOKENS,
                    )
                    .await
                {
                    context_recovery_attempts = context_recovery_attempts.saturating_add(1);
                    continue;
                }
            }
        }

        host.flush_pending_lsp_diagnostics().await;
        host.layered_context_checkpoint().await;

        let stream_span = tracing::info_span!(
            "turn_streaming",
            turn_id = %turn.id,
            step = turn.step,
        );
        let stream_out = async {
            host.run_streaming_phase(
                turn,
                client.as_ref(),
                mode,
                &tool_catalog,
                &active_tool_names,
                force_update_plan_first,
                &mut stream_retry_attempts,
                &mut context_recovery_attempts,
                &mut turn_error,
            )
            .await
        }
        .instrument(stream_span)
        .await;

        if let Some((status, err)) = stream_out.return_early {
            return (status, err);
        }
        if stream_out.break_outer_loop {
            break;
        }
        if stream_out.continue_outer_loop {
            continue;
        }

        let mut tool_uses = stream_out.tool_uses;
        let mut pending_steers = stream_out.pending_steers;

        let tools_span = tracing::info_span!(
            "turn_tools",
            turn_id = %turn.id,
            step = turn.step,
        );
        let phase = async {
            host.run_tool_execution_phase(
                turn,
                mode,
                &mut tool_uses,
                &tool_catalog,
                &mut active_tool_names,
                &mut loop_guard,
                consecutive_tool_error_steps,
                tool_registry,
            )
            .await
        }
        .instrument(tools_span)
        .await;

        if phase.break_outer_loop {
            break;
        }

        if phase.continue_outer_loop {
            if phase.step_error_count > 0 {
                consecutive_tool_error_steps = consecutive_tool_error_steps.saturating_add(1);
            } else {
                consecutive_tool_error_steps = 0;
            }
            turn.next_step();
            continue;
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
            consecutive_tool_error_steps = consecutive_tool_error_steps.saturating_add(1);
        } else {
            consecutive_tool_error_steps = 0;
        }

        if host
            .run_capacity_error_escalation_checkpoint(
                turn,
                mode,
                phase.step_error_count,
                consecutive_tool_error_steps,
                &phase.step_error_categories,
            )
            .await
        {
            turn.next_step();
            continue;
        }

        host.maybe_inject_scratchpad_reminder().await;

        turn.next_step();
    }

    if host.cancel_token().is_cancelled() {
        return (TurnOutcomeStatus::Interrupted, None);
    }
    if let Some(err) = turn_error {
        return (TurnOutcomeStatus::Failed, Some(err));
    }
    (TurnOutcomeStatus::Completed, None)
}
