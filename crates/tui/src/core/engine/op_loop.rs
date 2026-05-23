//! Engine background task: [`Op`] dispatch loop.

use super::*;

impl Engine {
    /// Run the engine event loop
    #[allow(clippy::too_many_lines)]
    pub async fn run(mut self) {
    while let Some(op) = self.rx_op.recv().await {
        match op {
            Op::SendMessage {
                content,
                mode,
                model,
                goal_objective,
                reasoning_effort,
                reasoning_effort_auto,
                auto_model,
                allow_shell,
                trust_mode,
                auto_approve,
                approval_mode,
            } => {
                self.handle_send_message(
                    content,
                    mode,
                    model,
                    goal_objective,
                    reasoning_effort,
                    reasoning_effort_auto,
                    auto_model,
                    allow_shell,
                    trust_mode,
                    auto_approve,
                    approval_mode,
                )
                .await;
            }
            Op::CancelRequest => {
                self.cancel_token.cancel();
                self.reset_cancel_token();
            }
            Op::ApproveToolCall { id } => {
                // Tool approval handling will be implemented in tools module
                let _ = self
                    .tx_event
                    .send(Event::status(format!("Approved tool call: {id}")))
                    .await;
            }
            Op::DenyToolCall { id } => {
                let _ = self
                    .tx_event
                    .send(Event::status(format!("Denied tool call: {id}")))
                    .await;
            }
            Op::SpawnSubAgent { prompt } => {
                match self.spawn_general_subagent(&prompt).await {
                    Ok(outcome) => {
                        let _ = self
                            .tx_event
                            .send(Event::status(format!(
                                "Spawned sub-agent {}",
                                outcome.agent_id
                            )))
                            .await;
                    }
                    Err(SubAgentSpawnError::NoClient) => {
                        let _ = self
                            .tx_event
                            .send(Event::error(ErrorEnvelope::fatal(
                                "Failed to spawn sub-agent: API client not configured",
                            )))
                            .await;
                    }
                    Err(SubAgentSpawnError::SpawnFailed(message)) => {
                        let _ = self
                            .tx_event
                            .send(Event::error(ErrorEnvelope::fatal(message)))
                            .await;
                    }
                }
            }
            Op::ListSubAgents => {
                let agents = {
                    let mut manager = self.subagent_manager.write().await;
                    manager.cleanup(Duration::from_secs(60 * 60));
                    manager.list()
                };
                let _ = self.tx_event.send(Event::AgentList { agents }).await;
            }
            Op::ChangeMode { mode } => {
                let _ = self
                    .tx_event
                    .send(Event::status(format!("Mode changed to: {mode:?}")))
                    .await;
            }
            Op::SetModel { model } => {
                self.session.auto_model = model.trim().eq_ignore_ascii_case("auto");
                self.session.model = model;
                self.config.model.clone_from(&self.session.model);
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "Model set to: {}",
                        self.session.model
                    )))
                    .await;
            }
            Op::SetCompaction { config } => {
                let enabled = config.enabled;
                self.config.compaction = config;
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "Auto-compaction {}",
                        if enabled { "enabled" } else { "disabled" }
                    )))
                    .await;
            }
            Op::SyncSession {
                messages,
                system_prompt,
                model,
                workspace,
            } => {
                self.session.messages = messages;
                self.session.compaction_summary_prompt =
                    extract_compaction_summary_prompt(system_prompt.clone());
                self.session.system_prompt = system_prompt;
                self.session.auto_model = model.trim().eq_ignore_ascii_case("auto");
                self.session.model = model;
                self.session.workspace = workspace.clone();
                self.config.model.clone_from(&self.session.model);
                self.config.workspace = workspace.clone();
                let ctx = crate::project_context::load_project_context_with_parents(&workspace);
                self.session.project_context = if ctx.has_instructions() {
                    Some(ctx)
                } else {
                    None
                };
                self.session.rebuild_working_set();
                self.rehydrate_latest_canonical_state();
                self.emit_session_updated().await;
                let _ = self
                    .tx_event
                    .send(Event::status("Session context synced".to_string()))
                    .await;
            }
            Op::CompactContext => {
                self.handle_manual_compaction().await;
            }
            Op::Rlm {
                content,
                model,
                child_model,
                max_depth,
            } => {
                self.handle_rlm(content, model, child_model, max_depth)
                    .await;
            }
            Op::EditLastTurn { new_message } => {
                // #383: /edit — remove the last user+assistant exchange
                // from the session, then re-send with the new content.
                // Pop messages from the tail until we've removed the
                // most recent user message and everything after it.
                // First, find the last user message index.
                let mut cut = None;
                for (idx, msg) in self.session.messages.iter().enumerate().rev() {
                    if msg.role == "user" {
                        cut = Some(idx);
                        break;
                    }
                }
                if let Some(idx) = cut {
                    self.session.messages.truncate(idx);
                }
                // Now dispatch the new message as a normal send,
                // reusing the engine's stored mode/model config.
                let mode = AppMode::Agent; // default fallback
                self.handle_send_message(
                    new_message,
                    mode,
                    self.session.model.clone(),
                    self.config.goal_objective.clone(),
                    self.session.reasoning_effort.clone(),
                    self.session.reasoning_effort_auto,
                    self.session.auto_model,
                    self.session.allow_shell,
                    self.session.trust_mode,
                    self.session.auto_approve,
                    self.session.approval_mode,
                )
                .await;
            }
            Op::QueryContext { reply } => {
                let snapshot = build_thread_context_snapshot(
                    &self.session.model,
                    &self.session.messages,
                    self.session.system_prompt.as_ref(),
                    &self.config.compaction,
                    Some(&self.session.workspace),
                    self.session.last_api_input_tokens,
                    None,
                    "engine",
                );
                let _ = reply.send(snapshot);
            }
            Op::Shutdown => {
                break;
            }
        }
    }

    // #420: graceful MCP shutdown — send SIGTERM and give stdio servers
    // a brief window to exit before drop fires SIGKILL via kill_on_drop.
    // Best-effort: pool may not exist (no MCP configured) and the lock
    // can fail under contention; either way the kill_on_drop fallback
    // still reaps the children.
    if let Some(pool) = self.mcp_pool.as_ref() {
        let mut guard = pool.lock().await;
        guard.shutdown_all().await;
    }
    }
}

