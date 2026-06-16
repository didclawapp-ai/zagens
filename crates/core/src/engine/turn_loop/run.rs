//! Outer turn-step loop for agent turns (P2 PR4 — generic over [`V3TurnHost`]).

use tracing::Instrument;

use crate::chat::Tool;
use crate::engine::context::summarize_text;
use crate::engine::kernel_event::{KernelEvent, TurnOutcome as KernelTurnOutcome};
use crate::engine::loop_guard::LoopGuard;
use crate::engine::turn_loop::live_turn_machine::{
    LiveOuterLoopState, LiveTurnMachine, OuterPostInnerStepOutcome, OuterPreInnerStepOutcome,
    OuterStepFrameOutcome, run_inner_step_via_machine, run_outer_post_inner_step_via_machine,
    run_outer_pre_inner_step_via_machine, run_outer_step_frame_via_machine,
};
use crate::engine::turn_machine::emit_kernel_event;
use crate::turn::{TurnContext, TurnLoopMode, TurnOutcomeStatus};

use super::host::V3TurnHost;
use super::turn_loop_outer_host::OuterLoopHost;

/// Run the multi-step agent turn loop until completion, interruption, or failure.
pub async fn handle_deepseek_turn<H: V3TurnHost>(
    host: &mut H,
    turn: &mut TurnContext,
    tool_registry: Option<&H::ToolRegistry>,
    tools: Option<Vec<Tool>>,
    mode: TurnLoopMode,
    force_update_plan_first: bool,
) -> (TurnOutcomeStatus, Option<String>) {
    tracing::info!(turn_id = %turn.id, "turn loop start");

    host.reset_kernel_turn_events();
    super::v3_driver::log_v3_turn_start(host, &turn.id);

    // Phase 3a double-write: emit TurnStarted.
    {
        let input_preview = host
            .session_mut()
            .messages
            .last()
            .and_then(|m| {
                m.content.iter().find_map(|b| {
                    if let crate::chat::ContentBlock::Text { text, .. } = b {
                        Some(summarize_text(text, 256))
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_default();
        emit_kernel_event(
            host,
            KernelEvent::TurnStarted {
                turn_id: turn.id.clone(),
                mode,
                input_text: input_preview,
                max_steps: turn.max_steps,
            },
        );
    }

    let mut loop_state = LiveOuterLoopState::default();
    let live_machine = LiveTurnMachine::default();

    let Some(client) = host.llm_client() else {
        let err = "DeepSeek client is not configured".to_string();
        end_turn(
            host,
            turn,
            &loop_state,
            KernelTurnOutcome::Failed {
                message: err.clone(),
            },
        )
        .await;
        return (TurnOutcomeStatus::Failed, Some(err));
    };

    let mut tool_catalog = tools.unwrap_or_default();
    if !tool_catalog.is_empty() {
        host.prepare_tool_catalog(&mut tool_catalog);
    }
    let mut active_tool_names = host.initial_active_tool_names(&tool_catalog);
    let mut loop_guard = LoopGuard::default();
    // Step-exhaustion continuation (LHT): grant another step-budget window when a
    let step_budget_increment = turn.max_steps.max(1);

    loop {
        tracing::debug!(turn_id = %turn.id, step = turn.step, "turn step");

        match run_outer_step_frame_via_machine(host, turn, &live_machine).await {
            OuterStepFrameOutcome::Proceed => {}
            OuterStepFrameOutcome::Cancelled => {
                end_turn(host, turn, &loop_state, KernelTurnOutcome::Interrupted).await;
                return (TurnOutcomeStatus::Interrupted, None);
            }
        }

        match run_outer_pre_inner_step_via_machine(
            host,
            turn,
            client.as_ref(),
            mode,
            &mut loop_state,
            &live_machine,
            step_budget_increment,
        )
        .await
        {
            OuterPreInnerStepOutcome::ContinueOuterLoop => continue,
            OuterPreInnerStepOutcome::BreakOuterLoop => break,
            OuterPreInnerStepOutcome::Failed => {
                let err = loop_state.turn_error.clone();
                end_turn(
                    host,
                    turn,
                    &loop_state,
                    KernelTurnOutcome::Failed {
                        message: err.clone().unwrap_or_else(|| "unknown error".to_string()),
                    },
                )
                .await;
                return (TurnOutcomeStatus::Failed, err);
            }
            OuterPreInnerStepOutcome::ProceedToInnerStep => {}
        }

        let stream_span = tracing::info_span!(
            "turn_streaming",
            turn_id = %turn.id,
            step = turn.step,
        );

        let v3 = async {
            run_inner_step_via_machine(
                host,
                turn,
                client.as_ref(),
                mode,
                &live_machine,
                &mut tool_catalog,
                &mut active_tool_names,
                force_update_plan_first,
                &mut loop_state,
                &mut loop_guard,
                tool_registry,
            )
            .await
        }
        .instrument(stream_span)
        .await;
        let stream_out = v3.stream;
        let phase = v3.tools;

        if let Some((status, err)) = stream_out.return_early {
            let outcome = match status {
                TurnOutcomeStatus::Interrupted => KernelTurnOutcome::Interrupted,
                TurnOutcomeStatus::Failed => KernelTurnOutcome::Failed {
                    message: err.clone().unwrap_or_else(|| "unknown error".to_string()),
                },
                TurnOutcomeStatus::Completed => KernelTurnOutcome::Completed,
            };
            end_turn(host, turn, &loop_state, outcome).await;
            return (status, err);
        }
        if stream_out.break_outer_loop {
            break;
        }
        if stream_out.continue_outer_loop {
            continue;
        }

        let mut pending_steers = stream_out.pending_steers;

        match run_outer_post_inner_step_via_machine(
            host,
            turn,
            mode,
            &mut loop_state,
            &live_machine,
            &phase,
            &mut pending_steers,
            &mut loop_guard,
        )
        .await
        {
            OuterPostInnerStepOutcome::ContinueOuterLoop => continue,
            OuterPostInnerStepOutcome::BreakOuterLoop => break,
            OuterPostInnerStepOutcome::AdvanceStep => {
                turn.next_step();
            }
        }
    }

    if host.cancel_token().is_cancelled() {
        end_turn(host, turn, &loop_state, KernelTurnOutcome::Interrupted).await;
        return (TurnOutcomeStatus::Interrupted, None);
    }
    if let Some(err) = loop_state.turn_error.clone() {
        end_turn(
            host,
            turn,
            &loop_state,
            KernelTurnOutcome::Failed {
                message: err.clone(),
            },
        )
        .await;
        return (TurnOutcomeStatus::Failed, Some(err));
    }
    // Defense-in-depth: every `break` above converges here as `Completed`,
    // regardless of whether a long-horizon task graph is actually finished.
    // Surface an incomplete give-up so the outcome isn't a silent false green.
    host.note_incomplete_stop_if_lht().await;
    end_turn(host, turn, &loop_state, KernelTurnOutcome::Completed).await;
    (TurnOutcomeStatus::Completed, None)
}

async fn end_turn<H: OuterLoopHost>(
    host: &mut H,
    turn: &TurnContext,
    loop_state: &LiveOuterLoopState,
    outcome: KernelTurnOutcome,
) {
    let scratchpad_summary_injected = *host.scratchpad_summary_injected_mut();
    emit_kernel_event(
        host,
        KernelEvent::TurnEnded {
            turn_id: turn.id.clone(),
            outcome,
            total_steps: turn.step,
        },
    );
    host.finish_kernel_turn(&loop_state.live_snapshot(turn, scratchpad_summary_injected))
        .await;
}
