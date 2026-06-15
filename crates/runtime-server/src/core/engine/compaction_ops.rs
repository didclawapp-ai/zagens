//! Manual context compaction from the op loop (P2 — `Op::CompactContext`).

use super::*;

use zagens_core::engine::kernel_event::{KernelEvent, MessageRange};
use zagens_core::engine::turn_machine::emit_kernel_event;

impl Engine {
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

    /// v3 effect interpreter entry for [`Effect::RunCompaction`].
    pub(in crate::core::engine) async fn run_compaction_effect(&mut self) {
        self.handle_manual_compaction().await;
    }
}
