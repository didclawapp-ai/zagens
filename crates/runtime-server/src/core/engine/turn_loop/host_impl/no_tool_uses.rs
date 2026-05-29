//! Steer / sub-agent / inline REPL when the model returns no tool calls (P2 PR6c).

use std::sync::Arc;

use deepseek_core::chat::{ContentBlock, Message};
use deepseek_core::engine::context::summarize_text;
use deepseek_core::engine::turn_loop::control::TurnLoopControl;
use deepseek_core::engine::TurnLoopHost;
use deepseek_core::turn::{TurnContext, TurnOutcomeStatus};

use super::super::Engine;
use crate::core::events::Event;

/// Drain any pending sub-agent completion notifications (non-blocking).
pub(super) fn drain_subagent_completions(
    rx: &Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<crate::tools::subagent::SubAgentCompletion>>>,
    out: &mut Vec<crate::tools::subagent::SubAgentCompletion>,
) {
    if let Ok(mut guard) = rx.try_lock() {
        while let Ok(c) = guard.try_recv() {
            out.push(c);
        }
    }
}

impl Engine {
    async fn maybe_inject_incomplete_audit_continue(&mut self) -> bool {
        if self.scratchpad_audit_continue_injected_this_turn {
            return false;
        }
        let Some(msg) = crate::core::engine::scratchpad_flow::maybe_continue_incomplete_audit(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
            &self.config.scratchpad,
            &self.session.messages,
        ) else {
            return false;
        };
        Engine::add_session_message(self, msg).await;
        self.scratchpad_audit_continue_injected_this_turn = true;
        let _ = self
            .tx_event
            .send(Event::status(
                "Audit scratchpad incomplete — continuing turn (P2 gates unmet)",
            ))
            .await;
        true
    }

    async fn maybe_inject_incomplete_lht_continue(&mut self, turn: &TurnContext) -> bool {
        if self.long_horizon_continue_injected_this_turn {
            return false;
        }

        let lh_config = self.config.long_horizon.clone();
        let scratchpad = self.config.scratchpad.clone();
        let task_type = self.config.task_type;
        let locale = self.config.locale_tag.clone();
        let workspace = self.session.workspace.clone();
        let run_id = self.scratchpad_run_id.clone();
        let messages = self.session.messages.clone();
        let plan_state = self.config_ext().plan_state.clone();
        let todos = self.config_ext().todos.clone();
        let app_mode = self.runtime_ext().turn_app_mode;
        let steps_remaining = turn.steps_remaining();

        self.runtime_ext_mut()
            .long_horizon_state
            .on_assistant_no_tools();
        let blocked_before = self
            .runtime_ext()
            .long_horizon_state
            .tracker
            .is_blocked();
        let converted_before = self
            .runtime_ext()
            .long_horizon_state
            .telemetry
            .converted;

        let input = crate::long_horizon::LongHorizonContinueInput {
            config: &lh_config,
            scratchpad: &scratchpad,
            task_type,
            app_mode,
            workspace: &workspace,
            scratchpad_run_id: run_id.as_deref(),
            messages: &messages,
            lang: &locale,
            plan_state: &plan_state,
            todos: &todos,
            session: &mut self.runtime_ext_mut().long_horizon_state,
            already_injected_this_turn: false,
            steps_remaining,
        };

        let gate = crate::long_horizon::maybe_continue_incomplete_code_task(input).await;

        // Telemetry (§4.9): emit a `nudge_outcome` whenever a prior nudge just
        // converted into qualified progress — the evidence we want for tuning.
        let converted_now = self
            .runtime_ext()
            .long_horizon_state
            .telemetry
            .converted;
        if converted_now > converted_before {
            let _ = self
                .tx_event
                .send(Event::status(format!(
                    "long_horizon.nudge_outcome: {{\"converted\":{converted_now}}}"
                )))
                .await;
        }

        let msg = match gate {
            crate::long_horizon::LhtGateOutcome::Nudge(msg) => msg,
            crate::long_horizon::LhtGateOutcome::Skip(reason) => {
                // §4.9 observability: emit exactly which guard suppressed the nudge,
                // alongside the engine-side state, so "it didn't fire" becomes
                // "it skipped at <reason> with <facts>" in a single run.
                let plan = plan_state.lock().await.snapshot();
                let todo = todos.lock().await.snapshot();
                let graph = crate::long_horizon::CodeTaskGraph::from_snapshots(&plan, &todo);
                let in_progress = graph
                    .in_progress_id
                    .map_or_else(|| "null".to_string(), |id| id.to_string());
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.gate_skip: {{\"reason\":\"{reason}\",\"enabled\":{},\"app_mode\":\"{:?}\",\"code_surface\":{},\"empty\":{},\"incomplete\":{},\"trivial\":{},\"in_progress_id\":{in_progress},\"open_items\":{}}}",
                        lh_config.enabled,
                        app_mode,
                        task_type.uses_code_tool_surface(),
                        graph.is_empty(),
                        graph.incomplete(),
                        graph.is_trivial(),
                        graph.open_items,
                    )))
                    .await;

                let blocked_now = self
                    .runtime_ext()
                    .long_horizon_state
                    .tracker
                    .is_blocked();
                if blocked_before || blocked_now {
                    let _ = self
                        .tx_event
                        .send(Event::status(format!(
                            "long_horizon.blocked: {{\"open_items\":{},\"reason\":\"max_nudges_without_progress\"}}",
                            graph.open_items
                        )))
                        .await;
                }
                return false;
            }
        };

        Engine::add_session_message(self, msg).await;
        self.long_horizon_continue_injected_this_turn = true;

        let plan = plan_state.lock().await.snapshot();
        let todo = todos.lock().await.snapshot();
        let open = crate::long_horizon::CodeTaskGraph::from_snapshots(&plan, &todo).open_items;
        let (nudge_count, emitted, converted) = {
            let lh = &self.runtime_ext().long_horizon_state;
            (
                lh.tracker.max_item_nudge_count(),
                lh.telemetry.emitted,
                lh.telemetry.converted,
            )
        };
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "long_horizon.continue_injected: {{\"open_items\":{open},\"nudge_count\":{nudge_count},\"emitted\":{emitted},\"converted\":{converted}}}"
            )))
            .await;
        true
    }

    pub(super) async fn handle_no_tool_uses_turn_loop(
        &mut self,
        turn: &mut TurnContext,
        pending_steers: &mut Vec<String>,
        current_text_visible: &str,
        has_sendable_assistant_content: bool,
    ) -> TurnLoopControl {
        if self.maybe_inject_scratchpad_summary().await && !pending_steers.is_empty() {
            for steer in pending_steers.drain(..) {
                let workspace = self.0.session.workspace.clone();
                self.0
                    .session
                    .working_set
                    .observe_user_message(&steer, &workspace);
                Engine::add_session_message(
                    self,
                    Message {
                        role: "user".to_string(),
                        content: vec![ContentBlock::Text {
                            text: steer,
                            cache_control: None,
                        }],
                    },
                )
                .await;
            }
            turn.next_step();
            return TurnLoopControl::Continue;
        }

        if !pending_steers.is_empty() {
            for steer in pending_steers.drain(..) {
                let workspace = self.0.session.workspace.clone();
                self.0
                    .session
                    .working_set
                    .observe_user_message(&steer, &workspace);
                Engine::add_session_message(
                    self,
                    Message {
                        role: "user".to_string(),
                        content: vec![ContentBlock::Text {
                            text: steer,
                            cache_control: None,
                        }],
                    },
                )
                .await;
            }
            turn.next_step();
            return TurnLoopControl::Continue;
        }

        let mut completions: Vec<crate::tools::subagent::SubAgentCompletion> = Vec::new();
        {
            let rx = Arc::clone(&self.runtime_ext().rx_subagent_completion);
            drain_subagent_completions(&rx, &mut completions);
        }
        if completions.is_empty() {
            // M3: route through `SubAgentHost::running_count` so the future
            // core-side Engine can swap `subagent_manager` to a trait object.
            let running = {
                let manager = Arc::clone(&self.runtime_ext().subagent_manager);
                manager.write().await.running_count()
            };
            if running > 0 {
                let rx = Arc::clone(&self.runtime_ext().rx_subagent_completion);
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "Waiting on {running} sub-agent(s) to complete..."
                    )))
                    .await;
                let cancel = self.0.cancel_token.clone();
                tokio::select! {
                    biased;
                    () = cancel.cancelled() => {
                        let _ = self
                            .tx_event
                            .send(Event::status(
                                "Request cancelled while waiting for sub-agents",
                            ))
                            .await;
                        return TurnLoopControl::Return(TurnOutcomeStatus::Interrupted, None);
                    }
                    Some(c) = {
                        let rx_wait = Arc::clone(&rx);
                        async move { rx_wait.lock().await.recv().await }
                    } => {
                        completions.push(c);
                        if let Ok(mut guard) = rx.try_lock() {
                            while let Ok(extra) = guard.try_recv() {
                                completions.push(extra);
                            }
                        }
                    }
                    Some(steer) = self.0.rx_steer.recv() => {
                        let trimmed = steer.trim().to_string();
                        if !trimmed.is_empty() {
                            self.runtime_ext_mut()
                                .long_horizon_state
                                .on_steer(&trimmed);
                            let workspace = self.0.session.workspace.clone();
                            self.0
                                .session
                                .working_set
                                .observe_user_message(&trimmed, &workspace);
                            Engine::add_session_message(
                                self,
                                Message {
                                    role: "user".to_string(),
                                    content: vec![ContentBlock::Text {
                                        text: trimmed.clone(),
                                        cache_control: None,
                                    }],
                                },
                            )
                            .await;
                            let _ = self.tx_event.send(Event::status(format!(
                                "Steer input accepted: {}",
                                summarize_text(&trimmed, 120)
                            ))).await;
                        }
                        turn.next_step();
                        return TurnLoopControl::Continue;
                    }
                }
            } else {
                // P1-8: completion may land after the first drain but before
                // running_count() observed zero — try once more.
                let rx = Arc::clone(&self.runtime_ext().rx_subagent_completion);
                drain_subagent_completions(&rx, &mut completions);
            }
        }
        if !completions.is_empty() {
            let count = completions.len();
            for c in completions {
                let workspace = self.0.session.workspace.clone();
                self.0
                    .session
                    .working_set
                    .observe_user_message(&c.payload, &workspace);
                Engine::add_session_message(
                    self,
                    Message {
                        role: "user".to_string(),
                        content: vec![ContentBlock::Text {
                            text: c.payload,
                            cache_control: None,
                        }],
                    },
                )
                .await;
            }
            let _ = self
                .tx_event
                .send(Event::status(format!(
                    "Resuming turn with {count} sub-agent completion(s)"
                )))
                .await;
            turn.next_step();
            return TurnLoopControl::Continue;
        }

        if has_sendable_assistant_content
            && crate::repl::sandbox::has_repl_block(current_text_visible)
        {
            let repl_blocks = crate::repl::sandbox::extract_repl_blocks(current_text_visible);
            let mut runtime = match crate::repl::runtime::PythonRuntime::new().await {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = self
                        .tx_event
                        .send(Event::status(format!("REPL init failed: {e}")))
                        .await;
                    return TurnLoopControl::Break;
                }
            };

            let mut final_result: Option<String> = None;
            for (i, block) in repl_blocks.iter().enumerate() {
                let round_num = i + 1;
                let _ = self
                    .tx_event
                    .send(Event::status(format!("REPL round {round_num}: executing...")))
                    .await;

                match runtime.execute(&block.code).await {
                    Ok(round) => {
                        if let Some(val) = &round.final_value {
                            let _ = self
                                .tx_event
                                .send(Event::status(format!(
                                    "REPL round {round_num}: FINAL result obtained"
                                )))
                                .await;
                            final_result = Some(val.clone());
                            break;
                        }

                        let feedback = if round.has_error {
                            format!(
                                "[REPL round {round_num} error]\nstdout:\n{}\nstderr:\n{}",
                                round.stdout, round.stderr
                            )
                        } else {
                            format!("[REPL round {round_num} output]\n{}", round.stdout)
                        };
                        Engine::add_session_message(
                            self,
                            Message {
                                role: "user".to_string(),
                                content: vec![ContentBlock::Text {
                                    text: feedback,
                                    cache_control: None,
                                }],
                            },
                        )
                        .await;
                    }
                    Err(e) => {
                        let _ = self
                            .tx_event
                            .send(Event::status(format!("REPL round {round_num} failed: {e}")))
                            .await;
                        Engine::add_session_message(
                            self,
                            Message {
                                role: "user".to_string(),
                                content: vec![ContentBlock::Text {
                                    text: format!("[REPL round {round_num} execution failed]\n{e}"),
                                    cache_control: None,
                                }],
                            },
                        )
                        .await;
                    }
                }
            }

            if let Some(final_val) = final_result {
                if let Some(last_msg) = self.session.messages.last_mut()
                    && last_msg.role == "assistant"
                {
                    for block in &mut last_msg.content {
                        if let ContentBlock::Text { text, .. } = block {
                            *text = final_val;
                            break;
                        }
                    }
                }
                Engine::emit_session_updated(self).await;
                return TurnLoopControl::Break;
            }

            turn.next_step();
            return TurnLoopControl::Continue;
        }

        if self.maybe_inject_incomplete_audit_continue().await {
            turn.next_step();
            return TurnLoopControl::Continue;
        }

        if self.maybe_inject_incomplete_lht_continue(turn).await {
            // LHT harness nudge: Continue without bumping step (§4.6).
            return TurnLoopControl::Continue;
        }

        TurnLoopControl::Break
    }
}

#[cfg(test)]
mod drain_tests {
    use std::sync::Arc;

    use tokio::sync::{Mutex, mpsc};

    use super::drain_subagent_completions;
    use crate::tools::subagent::SubAgentCompletion;

    #[test]
    fn second_drain_captures_completion_after_empty_first_drain() {
        let (tx, rx) = mpsc::unbounded_channel();
        let rx = Arc::new(Mutex::new(rx));
        let mut first = Vec::new();
        drain_subagent_completions(&rx, &mut first);
        assert!(first.is_empty());

        tx.send(SubAgentCompletion {
            agent_id: "agent_test".into(),
            payload: "done".into(),
        })
        .expect("send completion");

        let mut second = Vec::new();
        drain_subagent_completions(&rx, &mut second);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].agent_id, "agent_test");
    }
}
