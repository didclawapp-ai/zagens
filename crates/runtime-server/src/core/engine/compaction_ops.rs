//! In-turn auto-compaction, capacity trim/handoff, and manual `/compact`.

use super::effect_interpreter::EffectInterpreter;
use super::*;

use zagens_core::capacity::CapacitySnapshot;
use zagens_core::engine::kernel_event::{KernelEvent, MessageRange};
use zagens_core::engine::turn_machine::{Effect, emit_kernel_event};
use zagens_core::turn::{TurnContext, TurnLoopMode};

/// Distinguishes live `RunCompaction` IO paths (replay uses a single effect kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::core::engine) enum RunCompactionScope {
    InTurnAuto,
    CapacityTrim,
    CapacityHandoff,
}

impl Engine {
    /// Route auto-compaction through v3 `RunCompaction` effect or legacy direct IO.
    pub(in crate::core::engine) async fn route_auto_compaction(
        &mut self,
        client: &dyn LlmClient,
        turn_id: &str,
    ) {
        if self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            tracing::info!(
                target: "kernel_v3",
                turn_id,
                step = self.runtime_ext().kernel_active_step,
                "v3 compaction: RunCompaction auto (effect plan)"
            );
            self.runtime_ext_mut().kernel_run_compaction_scope =
                Some(RunCompactionScope::InTurnAuto);
            let mut interpreter = EffectInterpreter::new(self);
            let _ = interpreter.interpret(Effect::RunCompaction).await;
            return;
        }
        self.execute_in_turn_auto_compaction(client, turn_id).await;
    }

    /// Auto-compaction IO during an active turn (shared by legacy path and v3 effect).
    pub(in crate::core::engine) async fn execute_in_turn_auto_compaction(
        &mut self,
        client: &dyn LlmClient,
        turn_id: &str,
    ) {
        let compaction_pins = self
            .session
            .working_set
            .pinned_message_indices(&self.session.messages, &self.session.workspace);
        let mut compaction_paths = self.session.working_set.top_paths(24);
        super::scratchpad_flow::extend_compaction_paths(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
            &mut compaction_paths,
        );

        let compaction_id = format!("compact_{}", &uuid::Uuid::new_v4().to_string()[..8]);
        Engine::emit_compaction_started(
            self,
            compaction_id.clone(),
            true,
            "Auto context compaction started".to_string(),
        )
        .await;
        let _ = self
            .tx_event
            .send(Event::status("Auto-compacting context...".to_string()))
            .await;
        let auto_messages_before = self.session.messages.len();
        self.fire_pre_compact(self.runtime_ext().turn_app_mode, false);
        match compact_messages_safe(
            client,
            &self.session.messages,
            &self.config.compaction,
            Some(&self.session.workspace),
            Some(&compaction_pins),
            Some(&compaction_paths),
        )
        .await
        {
            Ok(result) => {
                if !result.messages.is_empty() || self.session.messages.is_empty() {
                    let auto_messages_after = result.messages.len();
                    self.session.messages = result.messages;
                    Engine::merge_compaction_summary(self, result.summary_prompt);
                    Engine::emit_session_updated(self).await;
                    let removed = auto_messages_before.saturating_sub(auto_messages_after);
                    let status = if result.retries_used > 0 {
                        format!(
                            "Auto-compaction complete: {auto_messages_before} → {auto_messages_after} messages ({removed} removed, {} retries)",
                            result.retries_used
                        )
                    } else {
                        format!(
                            "Auto-compaction complete: {auto_messages_before} → {auto_messages_after} messages ({removed} removed)"
                        )
                    };
                    Engine::emit_compaction_completed(
                        self,
                        compaction_id.clone(),
                        true,
                        status.clone(),
                        Some(auto_messages_before),
                        Some(auto_messages_after),
                    )
                    .await;
                    self.fire_post_compact(
                        self.runtime_ext().turn_app_mode,
                        false,
                        auto_messages_before,
                        auto_messages_after,
                    );
                    if let Some(artifact) = result.artifact {
                        emit_kernel_event(
                            self,
                            KernelEvent::CompactionArtifactCreated {
                                turn_id: turn_id.to_string(),
                                artifact_id: artifact.id,
                                replaced_range: MessageRange {
                                    from: artifact.replaced_start as u32,
                                    to: artifact
                                        .replaced_end
                                        .saturating_sub(1)
                                        .max(artifact.replaced_start)
                                        as u32,
                                },
                                summary_token_count: artifact.summary_tokens,
                            },
                        );
                    }
                    let _ = self.tx_event.send(Event::status(status)).await;
                } else {
                    let message = "Auto-compaction skipped: empty result".to_string();
                    Engine::emit_compaction_failed(
                        self,
                        compaction_id.clone(),
                        true,
                        message.clone(),
                    )
                    .await;
                    let _ = self.tx_event.send(Event::status(message)).await;
                }
            }
            Err(err) => {
                let message = format!("Auto-compaction failed: {err}");
                Engine::emit_compaction_failed(self, compaction_id, true, message.clone()).await;
                let _ = self.tx_event.send(Event::status(message)).await;
            }
        }
    }

    pub(super) async fn handle_manual_compaction(&mut self) {
        let id = format!("compact_{}", &uuid::Uuid::new_v4().to_string()[..8]);
        let zero_usage = Usage {
            input_tokens: 0,
            output_tokens: 0,
            ..Usage::default()
        };
        let Some(client) = self.deepseek_client.clone() else {
            let message = "Manual compaction unavailable: API client not configured".to_string();
            self.emit_compaction_failed(id, false, message.clone())
                .await;
            let _ = self
                .tx_event
                .send(Event::error(ErrorEnvelope::fatal_auth(message.clone())))
                .await;
            let _ = self
                .tx_event
                .send(Event::TurnComplete {
                    usage: zero_usage,
                    last_request_input_tokens: self.session.last_api_input_tokens,
                    status: TurnOutcomeStatus::Failed,
                    error: Some(message.clone()),
                    step_count: 0,
                    tool_names: vec![],
                    end_reason: Some(message),
                })
                .await;
            return;
        };

        let start_message = "Manual context compaction started".to_string();
        self.emit_compaction_started(id.clone(), false, start_message)
            .await;

        let compaction_pins = self
            .session
            .working_set
            .pinned_message_indices(&self.session.messages, &self.session.workspace);
        let compaction_paths = self.session.working_set.top_paths(24);
        let messages_before = self.session.messages.len();
        self.fire_pre_compact(self.runtime_ext().turn_app_mode, true);
        let mut turn_status = TurnOutcomeStatus::Completed;
        let mut turn_error = None;

        match compact_messages_safe(
            client.as_ref(),
            &self.session.messages,
            &self.config.compaction,
            Some(&self.session.workspace),
            Some(&compaction_pins),
            Some(&compaction_paths),
        )
        .await
        {
            Ok(result) => {
                if !result.messages.is_empty() || self.session.messages.is_empty() {
                    let messages_after = result.messages.len();
                    self.session.messages = result.messages;
                    self.merge_compaction_summary(result.summary_prompt);
                    self.emit_session_updated().await;
                    let removed = messages_before.saturating_sub(messages_after);
                    let message = if result.retries_used > 0 {
                        format!(
                            "Compaction complete: {messages_before} → {messages_after} messages ({removed} removed, {} retries)",
                            result.retries_used
                        )
                    } else {
                        format!(
                            "Compaction complete: {messages_before} → {messages_after} messages ({removed} removed)"
                        )
                    };
                    let compaction_id = id.clone();
                    self.emit_compaction_completed(
                        id,
                        false,
                        message,
                        Some(messages_before),
                        Some(messages_after),
                    )
                    .await;
                    if let Some(artifact) = result.artifact {
                        let turn_id = self
                            .runtime_ext()
                            .kernel_active_turn_id
                            .clone()
                            .unwrap_or_else(|| format!("manual-{compaction_id}"));
                        emit_kernel_event(
                            self,
                            KernelEvent::CompactionArtifactCreated {
                                turn_id,
                                artifact_id: artifact.id,
                                replaced_range: MessageRange {
                                    from: artifact.replaced_start as u32,
                                    to: artifact
                                        .replaced_end
                                        .saturating_sub(1)
                                        .max(artifact.replaced_start)
                                        as u32,
                                },
                                summary_token_count: artifact.summary_tokens,
                            },
                        );
                    }
                    self.fire_post_compact(
                        self.runtime_ext().turn_app_mode,
                        true,
                        messages_before,
                        messages_after,
                    );
                } else {
                    let message = "Compaction skipped: produced empty result".to_string();
                    self.emit_compaction_failed(id, false, message.clone())
                        .await;
                    turn_status = TurnOutcomeStatus::Failed;
                    turn_error = Some(message);
                }
            }
            Err(err) => {
                let message = format!("Manual context compaction failed: {err}");
                self.emit_compaction_failed(id, false, message.clone())
                    .await;
                let _ = self.tx_event.send(Event::status(message.clone())).await;
                turn_status = TurnOutcomeStatus::Failed;
                turn_error = Some(message);
            }
        }

        let _ = self
            .tx_event
            .send(Event::TurnComplete {
                usage: zero_usage,
                last_request_input_tokens: self.session.last_api_input_tokens,
                status: turn_status,
                error: turn_error.clone(),
                step_count: 0,
                tool_names: vec![],
                end_reason: turn_error,
            })
            .await;
    }

    fn capacity_turn_from_active_frame(&self) -> TurnContext {
        TurnContext {
            id: self
                .runtime_ext()
                .kernel_active_turn_id
                .clone()
                .unwrap_or_else(|| self.session.id.clone()),
            step: self.runtime_ext().kernel_active_step,
            max_steps: 0,
            tool_calls: Vec::new(),
            cancelled: false,
            usage: Usage::default(),
        }
    }

    /// v3 effect interpreter entry for compaction-like context reduction (not manual `/compact`).
    pub(in crate::core::engine) async fn run_compaction_effect(&mut self) {
        let scope = self
            .runtime_ext_mut()
            .kernel_run_compaction_scope
            .take()
            .unwrap_or(RunCompactionScope::InTurnAuto);
        if self.effect_replay_anchor_only() {
            tracing::info!(
                target: "kernel_v3",
                ?scope,
                "replay anchor-only: skipping RunCompaction IO"
            );
            if matches!(
                scope,
                RunCompactionScope::CapacityTrim | RunCompactionScope::CapacityHandoff
            ) {
                self.runtime_ext_mut().kernel_capacity_intervention_ok = Some(true);
            }
            return;
        }
        match scope {
            RunCompactionScope::InTurnAuto => {
                let turn_id = self
                    .runtime_ext()
                    .kernel_active_turn_id
                    .clone()
                    .unwrap_or_else(|| self.session.id.clone());
                let Some(client) = self.deepseek_client.clone() else {
                    tracing::warn!(
                        target: "kernel_v3",
                        turn_id = %turn_id,
                        "RunCompaction skipped: API client not configured"
                    );
                    return;
                };
                self.execute_in_turn_auto_compaction(client.as_ref(), &turn_id)
                    .await;
            }
            RunCompactionScope::CapacityTrim => {
                let turn = self.capacity_turn_from_active_frame();
                let mode = self
                    .runtime_ext_mut()
                    .kernel_capacity_turn_mode
                    .take()
                    .unwrap_or(TurnLoopMode::Agent);
                let snapshot = self.runtime_ext_mut().kernel_capacity_snapshot.take();
                let client = self.deepseek_client.clone();
                let ok = self
                    .apply_targeted_context_refresh(
                        &turn,
                        client.as_deref(),
                        mode,
                        snapshot.as_ref(),
                    )
                    .await;
                self.runtime_ext_mut().kernel_capacity_intervention_ok = Some(ok);
            }
            RunCompactionScope::CapacityHandoff => {
                let turn = self.capacity_turn_from_active_frame();
                let mode = self
                    .runtime_ext_mut()
                    .kernel_capacity_turn_mode
                    .take()
                    .unwrap_or(TurnLoopMode::Agent);
                let snapshot = self.runtime_ext_mut().kernel_capacity_snapshot.take();
                let reason = self
                    .runtime_ext_mut()
                    .kernel_capacity_handoff_reason
                    .take()
                    .unwrap_or_else(|| "capacity_handoff".to_string());
                let ok = self
                    .apply_verify_and_replan(&turn, mode, snapshot.as_ref(), &reason)
                    .await;
                self.runtime_ext_mut().kernel_capacity_intervention_ok = Some(ok);
            }
        }
    }
}
