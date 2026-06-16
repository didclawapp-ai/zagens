//! Outer turn-step loop for agent turns (P2 PR4 — generic over [`TurnLoopHost`]).

use tracing::Instrument;

use crate::chat::{ContentBlock, Message, Tool};
use crate::engine::context::{TURN_MAX_OUTPUT_TOKENS, context_input_budget, summarize_text};
use crate::engine::kernel_event::{KernelEvent, TurnOutcome as KernelTurnOutcome};
use crate::engine::loop_guard::LoopGuard;
use crate::engine::turn_loop::continuation_boundary_policy::{
    OuterBoundaryCounters, OuterBoundaryKind, context_overflow_budget_recompile_strategy,
    context_overflow_cycle_handoff_strategy, context_overflow_hard_fail_message,
    context_recovery_attempts_exhausted, context_recovery_budget_after_cycle_handoff,
    cycle_handoff_boundary_eligible, in_turn_cycle_advance_boundary_eligible,
    loop_guard_boundary_eligible, max_loop_guard_grants, max_step_limit_grants,
    step_limit_boundary_eligible, step_limit_budget_after_grant,
};
use crate::engine::turn_machine::{LiveTurnSnapshot, emit_kernel_event};
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

    host.reset_kernel_projection_shadow();
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

    let Some(client) = host.llm_client() else {
        let err = "DeepSeek client is not configured".to_string();
        end_turn(
            host,
            turn,
            0,
            0,
            0,
            0,
            KernelTurnOutcome::Failed {
                message: err.clone(),
            },
        )
        .await;
        return (TurnOutcomeStatus::Failed, Some(err));
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
    let mut length_continuations: u32 = 0;
    // Step-exhaustion continuation (LHT): grant another step-budget window when a
    // long-horizon task hits `max_steps` mid-flight, instead of silently stopping.
    let step_budget_increment = turn.max_steps.max(1);
    let mut step_limit_continuations: u32 = 0;
    // Loop-guard-halt continuation (LHT): when a tool fails enough times in a
    // row that `LoopGuard` halts the turn, give an incomplete long-horizon task
    // a bounded "change approach" continuation instead of silently completing.
    let mut loop_guard_continuations: u32 = 0;
    // Context-overflow cycle handoff (LHT): when the request grows past the
    // model budget and emergency compaction can't recover it, roll a cycle
    // handoff (briefing seed + preserved state) instead of hard-failing the
    // turn and dumping a manual `/compact` on the user.
    let mut cycle_handoff_attempts: u32 = 0;
    // Clean in-turn cycle advances (LHT #5): the cycle threshold / early-advance
    // gate is normally only checked between turns; evaluate it at each per-step
    // safe boundary so a long turn crossing ~75% gets a clean refresh instead of
    // only the hard-overflow fallback. Bounded so a pathological seed can't loop.
    let mut in_turn_cycle_advances: u32 = 0;

    loop {
        tracing::debug!(turn_id = %turn.id, step = turn.step, "turn step");

        host.reset_scratchpad_step();
        host.sync_kernel_turn_frame(turn);

        if host.cancel_token().is_cancelled() {
            let _ = host
                .tx_event()
                .send(Event::status("Request cancelled"))
                .await;
            end_turn(
                host,
                turn,
                step_limit_continuations,
                loop_guard_continuations,
                cycle_handoff_attempts,
                in_turn_cycle_advances,
                KernelTurnOutcome::Interrupted,
            )
            .await;
            return (TurnOutcomeStatus::Interrupted, None);
        }

        while let Ok(steer) = host.rx_steer_mut().try_recv() {
            host.inject_live_steer(turn, steer).await;
        }

        host.refresh_system_prompt(mode).await;
        host.maybe_lht_pre_request_hooks(mode).await;

        if turn.at_max_steps() {
            let boundary_counters = OuterBoundaryCounters {
                step_limit_continuations,
                loop_guard_continuations,
                cycle_handoff_attempts,
                in_turn_cycle_advances,
            };
            if step_limit_boundary_eligible(mode, boundary_counters)
                && host.maybe_continue_at_step_limit(turn).await
            {
                step_limit_continuations = step_limit_continuations.saturating_add(1);
                turn.max_steps = step_limit_budget_after_grant(turn, step_budget_increment);
                let _ = host
                    .tx_event()
                    .send(Event::status(format!(
                        "Step budget reached; continuing long-horizon task ({}/{})",
                        step_limit_continuations,
                        max_step_limit_grants()
                    )))
                    .await;
                emit_kernel_event(
                    host,
                    KernelEvent::StepLimitContinuation {
                        turn_id: turn.id.clone(),
                        step_idx: turn.step,
                        lht_objective_injected: true,
                    },
                );
                super::v3_driver::log_v3_outer_boundary(
                    host,
                    OuterBoundaryKind::StepLimit,
                    &turn.id,
                    turn.step,
                    step_limit_continuations,
                );
                continue;
            }
            let _ = host
                .tx_event()
                .send(Event::status("Reached maximum steps"))
                .await;
            break;
        }

        if host.kernel_machine_mode().uses_v3_turn_loop() {
            super::v3_driver::log_v3_pre_inner_step_plan(host, &turn.id, turn.step);
        }

        host.run_pre_inner_step_auto_compaction(client.as_ref(), turn)
            .await;

        if host
            .run_capacity_pre_request_checkpoint(turn, Some(client.as_ref()), mode)
            .await
        {
            super::v3_driver::log_v3_outer_boundary(
                host,
                OuterBoundaryKind::PreRequestCapacityHold,
                &turn.id,
                turn.step,
                turn.step,
            );
            continue;
        }

        let model = host.session_mut().model.clone();
        if let Some(input_budget) = context_input_budget(&model, TURN_MAX_OUTPUT_TOKENS) {
            let estimated_input = host.estimated_input_tokens();
            if estimated_input > input_budget {
                if context_recovery_attempts_exhausted(context_recovery_attempts) {
                    let overflow_counters = OuterBoundaryCounters {
                        step_limit_continuations,
                        loop_guard_continuations,
                        cycle_handoff_attempts,
                        in_turn_cycle_advances,
                    };
                    if cycle_handoff_boundary_eligible(mode, overflow_counters)
                        && host
                            .maybe_cycle_handoff_on_context_overflow(turn, mode)
                            .await
                    {
                        cycle_handoff_attempts = cycle_handoff_attempts.saturating_add(1);
                        emit_kernel_event(
                            host,
                            KernelEvent::ContextOverflowRecovered {
                                turn_id: turn.id.clone(),
                                step_idx: turn.step,
                                strategy: context_overflow_cycle_handoff_strategy(),
                                source_budget_cap: Some(input_budget.min(u32::MAX as usize) as u32),
                            },
                        );
                        context_recovery_attempts = context_recovery_budget_after_cycle_handoff();
                        super::v3_driver::log_v3_outer_boundary(
                            host,
                            OuterBoundaryKind::ContextOverflowCycleHandoff,
                            &turn.id,
                            turn.step,
                            cycle_handoff_attempts,
                        );
                        continue;
                    }
                    let message = context_overflow_hard_fail_message(estimated_input, input_budget);
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
                    emit_kernel_event(
                        host,
                        KernelEvent::ContextOverflowRecovered {
                            turn_id: turn.id.clone(),
                            step_idx: turn.step,
                            strategy: context_overflow_budget_recompile_strategy(),
                            source_budget_cap: Some(input_budget.min(u32::MAX as usize) as u32),
                        },
                    );
                    continue;
                }
            }
        }

        // v3 routes LSP drain through `Effect::NotifyLsp` after each tool batch (batch 6t);
        // pre-step flush is redundant and would double-inject if blocks were still pending.
        if !host.kernel_machine_mode().uses_v3_turn_loop() {
            host.flush_pending_lsp_diagnostics().await;
        }
        // v3 routes Flash seam checkpoint through planner baseline slot 1 (batch 5b).
        host.run_pre_inner_step_layered_context().await;

        let stream_span = tracing::info_span!(
            "turn_streaming",
            turn_id = %turn.id,
            step = turn.step,
        );

        let (stream_out, phase) = if host.kernel_machine_mode().uses_v3_turn_loop() {
            let v3 = async {
                super::v3_step::run_v3_turn_step_unified(
                    host,
                    turn,
                    client.as_ref(),
                    mode,
                    &mut tool_catalog,
                    &mut active_tool_names,
                    force_update_plan_first,
                    &mut stream_retry_attempts,
                    &mut context_recovery_attempts,
                    &mut length_continuations,
                    &mut turn_error,
                    &mut loop_guard,
                    consecutive_tool_error_steps,
                    tool_registry,
                )
                .await
            }
            .instrument(stream_span)
            .await;
            (v3.stream, v3.tools)
        } else {
            let stream_out = async {
                super::streaming_phase::run_streaming_phase(
                    host,
                    turn,
                    client.as_ref(),
                    mode,
                    &tool_catalog,
                    &active_tool_names,
                    force_update_plan_first,
                    &mut stream_retry_attempts,
                    &mut context_recovery_attempts,
                    &mut length_continuations,
                    &mut turn_error,
                )
                .await
            }
            .instrument(stream_span)
            .await;

            let mut tool_uses = stream_out.tool_uses;
            let pending_steers = stream_out.pending_steers;
            let continue_outer_loop = stream_out.continue_outer_loop;
            let break_outer_loop = stream_out.break_outer_loop;
            let return_early = stream_out.return_early;
            let tools_span = tracing::info_span!(
                "turn_tools",
                turn_id = %turn.id,
                step = turn.step,
            );
            let phase = async {
                super::tool_phase::run_tool_execution_phase(
                    host,
                    turn,
                    mode,
                    &mut tool_uses,
                    &mut tool_catalog,
                    &mut active_tool_names,
                    &mut loop_guard,
                    consecutive_tool_error_steps,
                    tool_registry,
                )
                .await
            }
            .instrument(tools_span)
            .await;
            (
                super::control::TurnLoopStreamingPhaseOutcome {
                    tool_uses,
                    pending_steers,
                    continue_outer_loop,
                    break_outer_loop,
                    return_early,
                },
                phase,
            )
        };

        if let Some((status, err)) = stream_out.return_early {
            return (status, err);
        }
        if stream_out.break_outer_loop {
            break;
        }
        if stream_out.continue_outer_loop {
            continue;
        }

        let mut pending_steers = stream_out.pending_steers;

        if phase.break_outer_loop {
            let boundary_counters = OuterBoundaryCounters {
                step_limit_continuations,
                loop_guard_continuations,
                cycle_handoff_attempts,
                in_turn_cycle_advances,
            };
            if loop_guard_boundary_eligible(mode, phase.loop_guard_halted, boundary_counters)
                && host.maybe_continue_after_loop_guard_halt(turn).await
            {
                loop_guard_continuations = loop_guard_continuations.saturating_add(1);
                loop_guard.reset_failures();
                let _ = host
                    .tx_event()
                    .send(Event::status(format!(
                        "Loop-guard halt; nudging long-horizon task to change approach ({}/{})",
                        loop_guard_continuations,
                        max_loop_guard_grants()
                    )))
                    .await;
                emit_kernel_event(
                    host,
                    KernelEvent::LoopGuardContinuation {
                        turn_id: turn.id.clone(),
                        step_idx: turn.step,
                    },
                );
                super::v3_driver::log_v3_outer_boundary(
                    host,
                    OuterBoundaryKind::LoopGuard,
                    &turn.id,
                    turn.step,
                    loop_guard_continuations,
                );
                turn.next_step();
                continue;
            }
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
            super::v3_driver::log_v3_outer_boundary(
                host,
                OuterBoundaryKind::ErrorEscalationCapacityHold,
                &turn.id,
                turn.step,
                turn.step,
            );
            turn.next_step();
            continue;
        }

        host.maybe_inject_scratchpad_reminder(turn).await;

        // Per-step safe boundary (#5): a long-horizon turn can loop many tool
        // steps without returning to the between-turns boundary where the cycle
        // gate is normally evaluated. Check the clean threshold / early-advance
        // gate here (stream + tools already finished → no in-flight cut). On a
        // handoff the buffer becomes a small briefing seed, so re-loop to
        // re-request with the fresh context. Bounded against pathological seeds.
        let cycle_counters = OuterBoundaryCounters {
            step_limit_continuations,
            loop_guard_continuations,
            cycle_handoff_attempts,
            in_turn_cycle_advances,
        };
        if in_turn_cycle_advance_boundary_eligible(mode, cycle_counters)
            && host.maybe_advance_cycle_at_checkpoint(mode, turn).await
        {
            in_turn_cycle_advances = in_turn_cycle_advances.saturating_add(1);
            super::v3_driver::log_v3_outer_boundary(
                host,
                OuterBoundaryKind::InTurnCycleAdvance,
                &turn.id,
                turn.step,
                in_turn_cycle_advances,
            );
            turn.next_step();
            continue;
        }

        turn.next_step();
    }

    if host.cancel_token().is_cancelled() {
        end_turn(
            host,
            turn,
            step_limit_continuations,
            loop_guard_continuations,
            cycle_handoff_attempts,
            in_turn_cycle_advances,
            KernelTurnOutcome::Interrupted,
        )
        .await;
        return (TurnOutcomeStatus::Interrupted, None);
    }
    if let Some(err) = turn_error {
        end_turn(
            host,
            turn,
            step_limit_continuations,
            loop_guard_continuations,
            cycle_handoff_attempts,
            in_turn_cycle_advances,
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
    end_turn(
        host,
        turn,
        step_limit_continuations,
        loop_guard_continuations,
        cycle_handoff_attempts,
        in_turn_cycle_advances,
        KernelTurnOutcome::Completed,
    )
    .await;
    (TurnOutcomeStatus::Completed, None)
}

fn live_turn_snapshot(
    turn: &TurnContext,
    scratchpad_summary_injected: bool,
    step_limit_continuations: u32,
    loop_guard_continuations: u32,
    cycle_handoff_attempts: u32,
    in_turn_cycle_advances: u32,
) -> LiveTurnSnapshot {
    LiveTurnSnapshot {
        turn_id: turn.id.clone(),
        step_idx: turn.step,
        max_steps: turn.max_steps,
        scratchpad_summary_injected,
        step_limit_continuations,
        loop_guard_continuations,
        cycle_handoff_attempts,
        in_turn_cycle_advances,
    }
}

async fn end_turn<H: TurnLoopHost>(
    host: &mut H,
    turn: &TurnContext,
    step_limit_continuations: u32,
    loop_guard_continuations: u32,
    cycle_handoff_attempts: u32,
    in_turn_cycle_advances: u32,
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
    host.finish_kernel_turn_shadow(&live_turn_snapshot(
        turn,
        scratchpad_summary_injected,
        step_limit_continuations,
        loop_guard_continuations,
        cycle_handoff_attempts,
        in_turn_cycle_advances,
    ))
    .await;
}
