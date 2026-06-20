//! User message handler (`Op::SendMessage`).

use crate::core::events::TurnSummary;

use super::*;

impl Engine {
    /// Handle a send message operation
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn handle_send_message(
        &mut self,
        content: String,
        mode: AppMode,
        model: String,
        goal_objective: Option<String>,
        reasoning_effort: Option<String>,
        reasoning_effort_auto: bool,
        auto_model: bool,
        allow_shell: bool,
        trust_mode: bool,
        auto_approve: bool,
        approval_mode: zagens_core::approval::ApprovalMode,
        temperature: Option<f32>,
        top_p: Option<f32>,
        max_output_tokens: Option<u32>,
        turn_id: Option<String>,
    ) {
        self.emit_pending_startup_warnings().await;

        // Reset cancel token for fresh turn (in case previous was cancelled)
        self.reset_cancel_token();

        // Drain stale steer messages from previous turns.
        while self.rx_steer.try_recv().is_ok() {}

        // Create turn context first so start event includes a stable turn id.
        let mut turn = match turn_id {
            Some(id) if !id.trim().is_empty() => TurnContext::with_id(self.config.max_steps, id),
            _ => TurnContext::new(self.config.max_steps),
        };
        self.0.turn_counter = self.0.turn_counter.saturating_add(1);
        self.0
            .capacity_controller
            .mark_turn_start(self.0.turn_counter);
        self.scratchpad_step.reset();
        self.scratchpad_summary_injected_this_turn = false;
        self.scratchpad_audit_continue_injected_this_turn = false;
        self.long_horizon_continue_injected_this_turn = false;
        self.long_horizon_auto_continue_rounds = 0;
        self.0.overflow_source_budget_cap = None;
        self.runtime_ext_mut().turn_app_mode = mode;
        self.runtime_ext_mut()
            .long_horizon_state
            .on_new_user_message();
        self.sync_scratchpad_run_id_from_wire();

        if !self.runtime_ext().workspace_preflight_done {
            let ws = self.config.workspace.clone();
            let report = tokio::task::spawn_blocking(move || {
                crate::tools::workspace_preflight::apply_windows_node_preflight(&ws)
            })
            .await
            .ok();
            if let Some(report) = report
                && let Some(status) =
                    crate::tools::workspace_preflight::format_preflight_status(&report)
            {
                let _ = self.tx_event.send(Event::status(status)).await;
            }
            self.runtime_ext_mut().workspace_preflight_done = true;
        }

        self.maybe_fire_session_start(mode);
        if let Err(blocked) = self.fire_message_submit(mode, &content) {
            let _ = self
                .tx_event
                .send(Event::error(ErrorEnvelope::fatal(blocked.clone())))
                .await;
            let _ = self
                .tx_event
                .send(Event::TurnComplete {
                    usage: turn.usage.clone(),
                    last_request_input_tokens: self.session.last_api_input_tokens,
                    status: TurnOutcomeStatus::Failed,
                    error: Some(blocked.clone()),
                    step_count: 0,
                    tool_names: vec![],
                    end_reason: Some(blocked),
                })
                .await;
            return;
        }

        // Snapshot the workspace BEFORE we touch a single tool. Run the git
        // work on the blocking pool so the async runtime stays responsive;
        // failure is non-fatal (the helper logs at WARN).
        if self.config.snapshots_enabled {
            let pre_workspace = self.session.workspace.clone();
            let pre_seq = self.turn_counter;
            let max_gb = self.config.snapshots_max_workspace_gb;
            let _ = tokio::task::spawn_blocking(move || {
                pre_turn_snapshot(&pre_workspace, pre_seq, max_gb)
            })
            .await;
        }

        // Emit turn started event
        let _ = self
            .tx_event
            .send(Event::TurnStarted {
                turn_id: turn.id.clone(),
            })
            .await;

        // A new turn means any leftover retry banner (success cleared
        // it, failure pinned it) is no longer relevant — reset to idle
        // so the footer doesn't display a stale failure row across
        // turns (#499).
        crate::retry_status::clear();

        // Check if we have the appropriate client
        if self.deepseek_client.is_none() {
            let message = self
                .deepseek_client_error
                .as_deref()
                .map(|err| format!("Failed to send message: {err}"))
                .unwrap_or_else(|| "Failed to send message: API client not configured".to_string());
            self.fire_on_error(mode, &message);
            let _ = self
                .tx_event
                .send(Event::error(ErrorEnvelope::fatal_auth(message.clone())))
                .await;
            let _ = self
                .tx_event
                .send(Event::TurnComplete {
                    usage: turn.usage.clone(),
                    last_request_input_tokens: self.session.last_api_input_tokens,
                    status: TurnOutcomeStatus::Failed,
                    error: Some(message.clone()),
                    step_count: 0,
                    tool_names: vec![],
                    end_reason: Some(message),
                })
                .await;
            return;
        }

        let workspace = self.0.session.workspace.clone();
        self.0
            .session
            .working_set
            .observe_user_message(&content, &workspace);
        let force_update_plan_first = should_force_update_plan_first(mode, &content);

        let inject_report_summary = self.config.scratchpad.enabled
            && scratchpad_flow::user_prompt_triggers_report_summary(
                &content,
                &self.config.scratchpad,
            );

        // Add user message to session
        let user_msg = Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: content,
                cache_control: None,
            }],
        };
        self.session.add_message(user_msg);

        if inject_report_summary
            && !self.scratchpad_summary_injected_this_turn
            && let Some(summary_msg) = scratchpad_flow::build_report_summary_message(
                &self.session.workspace,
                self.scratchpad_run_id.as_deref(),
                &self.config.scratchpad,
            )
        {
            self.session.add_message(summary_msg);
            self.scratchpad_summary_injected_this_turn = true;
        }

        self.0.session.model = model;
        self.0.config.model.clone_from(&self.0.session.model);
        self.0.config.goal_objective = goal_objective;
        self.session.reasoning_effort = reasoning_effort;
        self.session.reasoning_effort_auto = reasoning_effort_auto;
        self.session.auto_model = auto_model;
        self.session.allow_shell = allow_shell;
        self.config.allow_shell = allow_shell;
        self.session.trust_mode = trust_mode;
        self.config.trust_mode = trust_mode;
        self.session.auto_approve = auto_approve;
        self.session.approval_mode = if auto_approve {
            zagens_core::approval::ApprovalMode::Auto
        } else {
            approval_mode
        };
        if temperature.is_some() {
            self.session.temperature = temperature;
        }
        if top_p.is_some() {
            self.session.top_p = top_p;
        }
        if max_output_tokens.is_some() {
            self.session.max_output_tokens = max_output_tokens;
        }

        // Update system prompt to match current mode and include persisted compaction context.
        self.refresh_system_prompt(mode);
        self.emit_session_updated().await;

        // Build tool registry and tool list for the current mode
        let todo_list = self.config_ext().todos.clone();
        let plan_state = self.config_ext().plan_state.clone();

        let tool_context = self.build_tool_context(mode, auto_approve);
        let builder = self.build_turn_tool_registry_builder(mode, todo_list, plan_state);

        // Mailbox for structured sub-agent envelopes (#128/#130). One per
        // turn: the receiver is drained by a short-lived task that converts
        // envelopes into `Event::SubAgentMailbox` so the UI can route them
        // to the matching in-transcript card. The drainer exits naturally
        // when every cloned sender is dropped at turn-end.
        let mailbox_for_runtime = if self.config.task_type.uses_code_tool_surface()
            && self.config.features.enabled(Feature::Subagents)
        {
            let cancel_token = self.cancel_token.child_token();
            let (mailbox, mut receiver) = Mailbox::new(cancel_token.clone());
            let tx_event_clone = self.tx_event.clone();
            spawn_supervised(
                "subagent-mailbox-drainer",
                std::panic::Location::caller(),
                async move {
                    while let Some(envelope) = receiver.recv().await {
                        if tx_event_clone
                            .send(Event::SubAgentMailbox {
                                seq: envelope.seq,
                                message: envelope.message,
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                },
            );
            Some((mailbox, cancel_token))
        } else {
            None
        };

        let tool_registry = match mode {
            AppMode::Agent | AppMode::Yolo => {
                if self.config.task_type.uses_code_tool_surface()
                    && self.config.features.enabled(Feature::Subagents)
                {
                    let runtime = if let Some(client) = self.deepseek_client.clone() {
                        let mut rt = SubAgentRuntime::new(
                            client,
                            self.session.model.clone(),
                            tool_context.clone(),
                            self.session.allow_shell,
                            Some(self.tx_event.clone()),
                            Arc::clone(&self.runtime_ext().subagent_manager),
                        )
                        .with_role_models(self.config.subagent_model_overrides.clone())
                        .with_auto_model(self.session.auto_model)
                        .with_reasoning_effort(
                            self.session.reasoning_effort.clone(),
                            self.session.reasoning_effort_auto,
                        )
                        .with_max_spawn_depth(self.config.max_spawn_depth)
                        .with_step_timeout(self.config.subagent_step_timeout)
                        .with_parent_completion_tx(
                            self.runtime_ext().tx_subagent_completion.clone(),
                        )
                        .with_hook_executor(Arc::clone(&self.runtime_ext().hook_executor));
                        if let Some((mailbox, cancel_token)) = mailbox_for_runtime.as_ref() {
                            rt = rt
                                .with_mailbox(mailbox.clone())
                                .with_cancel_token(cancel_token.clone());
                        }
                        Some(rt)
                    } else {
                        None
                    };
                    // If runtime is absent (no ConfigStore client), skip sub-agent
                    // tools gracefully instead of panicking (#D2).
                    if let Some(runtime) = runtime {
                        Some(
                            builder
                                .with_subagent_tools(
                                    self.runtime_ext().subagent_manager.clone(),
                                    runtime,
                                )
                                .build(tool_context),
                        )
                    } else {
                        Some(builder.build(tool_context))
                    }
                } else {
                    Some(builder.build(tool_context))
                }
            }
            _ => Some(builder.build(tool_context)),
        };

        let mcp_tools = if self.config.task_type.uses_code_tool_surface()
            && self.config.features.enabled(Feature::Mcp)
        {
            self.mcp_tools().await
        } else {
            Vec::new()
        };
        let tools = tool_registry.as_ref().map(|registry| {
            build_model_tool_catalog(
                registry.to_api_tools_with_cache(true),
                mcp_tools,
                mode,
                self.scratchpad_run_id.as_deref(),
            )
        });

        // Main turn loop
        let (status, error) = self
            .handle_deepseek_turn(
                &mut turn,
                tool_registry.as_ref(),
                tools,
                mode,
                force_update_plan_first,
            )
            .await;

        // Checkpoint-restart cycle boundary (issue #124). Run BEFORE
        // TurnComplete so the engine loop doesn't block the terminal after
        // the turn signal (#234). The status chip ("↻ context refreshing...")
        // is visible during the wait, and once TurnComplete fires the
        // terminal is immediately responsive. No-op unless the estimated
        // input tokens have crossed the per-cycle threshold.
        if matches!(status, TurnOutcomeStatus::Completed) {
            if let Some((user, assistant)) =
                crate::topic_memory::last_exchange_from_messages(&self.session.messages)
            {
                // M5: dispatch through TopicMemoryHost — settings owned
                // by the runtime (set at Engine::new from
                // config.topic_memory).
                use zagens_core::engine::hosts::TopicMemoryHost;
                TopicMemoryHost::on_turn_complete(&mut *self.topic_memory, &user, &assistant);
            }
            self.maybe_advance_cycle(mode, None).await;
        }

        // Update session usage
        self.session.total_usage.add(&turn.usage);

        // Emit turn complete event — after all post-turn bookkeeping so
        // the terminal is immediately responsive when the UI receives it.
        let end_reason: Option<String> = match status {
            TurnOutcomeStatus::Completed => None,
            TurnOutcomeStatus::Interrupted => Some("cancelled".to_string()),
            TurnOutcomeStatus::Failed => {
                Some(error.as_deref().unwrap_or("unknown error").to_string())
            }
        };
        let mut tool_names: Vec<String> =
            turn.tool_calls.iter().map(|tc| tc.name.clone()).collect();
        tool_names.sort();
        tool_names.dedup();
        let summary = TurnSummary::new(turn.step, tool_names.clone(), end_reason.clone());
        summary.log_turn_complete(&turn.id, status, None);
        if matches!(status, TurnOutcomeStatus::Failed)
            && let Some(ref err) = error
        {
            self.fire_on_error(mode, err);
        }
        let _ = self
            .tx_event
            .send(Event::TurnComplete {
                usage: turn.usage,
                last_request_input_tokens: self.session.last_api_input_tokens,
                status,
                error,
                step_count: summary.step_count,
                tool_names: summary.tool_names,
                end_reason: summary.end_reason,
            })
            .await;

        // Post-turn snapshot. Fire-and-forget: TurnComplete is already
        // emitted, so the UI is unblocked and the user can type / select /
        // paste immediately (#234). The git work proceeds on the blocking
        // pool without forcing the engine loop to await it.
        if self.config.snapshots_enabled {
            let post_workspace = self.session.workspace.clone();
            let post_seq = self.turn_counter;
            let max_gb = self.config.snapshots_max_workspace_gb;
            crate::utils::spawn_blocking_supervised("post-turn-snapshot", move || {
                post_turn_snapshot(&post_workspace, post_seq, max_gb);
            });
        }
    }
}
