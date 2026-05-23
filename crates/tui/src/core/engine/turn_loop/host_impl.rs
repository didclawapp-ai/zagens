//! `TurnLoopHost` implementation for the TUI `Engine` (P2 PR4 step 2).

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use deepseek_core::chat::{LlmClient, Message, Tool};
use deepseek_core::engine::context::{estimate_input_tokens_conservative, summarize_text};
use deepseek_core::chat::ContentBlock;
use deepseek_core::engine::turn_loop::control::{
    TurnLoopControl, TurnLoopStreamingPhaseOutcome, TurnLoopToolPhaseOutcome,
};
use deepseek_core::engine::turn_loop::{TurnLoopMcpPool, TurnLoopToolRegistry};
use deepseek_core::engine::TurnLoopHost;
use deepseek_core::engine::loop_guard::LoopGuard;
use deepseek_core::engine::streaming::ToolUseState;
use deepseek_core::turn::{TurnContext, TurnLoopMode, TurnOutcomeStatus};
use deepseek_tools::{ToolError, ToolResult};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex as AsyncMutex, RwLock};

use super::super::tool_catalog::{
    active_tools_for_step, ensure_advanced_tooling, execute_code_execution_tool,
    execute_tool_search, initial_active_tools, maybe_activate_requested_deferred_tool,
};
use super::super::scratchpad_flow;
use super::Engine;
use crate::compaction::{compact_messages_safe, should_compact};
use crate::core::events::Event;
use crate::core::turn::pre_tool_snapshot;
use crate::mcp::McpPool;
use crate::tui::app::AppMode;
use crate::tools::ToolRegistry;
impl TurnLoopToolRegistry for ToolRegistry {}
impl TurnLoopMcpPool for McpPool {}

#[async_trait]
impl TurnLoopHost for Engine {
    type ToolRegistry = ToolRegistry;
    type McpPool = McpPool;

    fn session_mut(&mut self) -> &mut deepseek_core::session::Session {
        &mut self.session
    }

    fn compaction_config(&self) -> &deepseek_core::compaction::CompactionConfig {
        &self.config.compaction
    }

    fn workspace(&self) -> &std::path::Path {
        &self.config.workspace
    }

    fn strict_tool_mode(&self) -> bool {
        self.config.strict_tool_mode
    }

    fn scratchpad_config(&self) -> &deepseek_core::scratchpad::ScratchpadConfig {
        &self.config.scratchpad
    }

    fn scratchpad_run_id(&self) -> Option<&str> {
        self.scratchpad_run_id.as_deref()
    }

    fn scratchpad_summary_injected_mut(&mut self) -> &mut bool {
        &mut self.scratchpad_summary_injected_this_turn
    }

    fn cancel_token(&self) -> &tokio_util::sync::CancellationToken {
        &self.cancel_token
    }

    fn tx_event(&self) -> &mpsc::Sender<Event> {
        &self.tx_event
    }

    fn rx_steer_mut(&mut self) -> &mut mpsc::Receiver<String> {
        &mut self.rx_steer
    }

    fn tool_exec_lock(&self) -> Arc<RwLock<()>> {
        self.tool_exec_lock.clone()
    }

    fn llm_client(&self) -> Option<Arc<dyn LlmClient>> {
        self.deepseek_client.clone()
    }

    fn prepare_tool_catalog(&self, catalog: &mut Vec<Tool>) {
        ensure_advanced_tooling(catalog);
    }

    fn initial_active_tool_names(&self, catalog: &[Tool]) -> HashSet<String> {
        initial_active_tools(catalog)
    }

    fn active_tools_for_step(
        &self,
        catalog: &[Tool],
        active: &HashSet<String>,
        force_update_plan_first: bool,
    ) -> Vec<Tool> {
        active_tools_for_step(catalog, active, force_update_plan_first)
    }

    fn is_mcp_tool_name(&self, name: &str) -> bool {
        McpPool::is_mcp_tool(name)
    }

    fn maybe_activate_deferred_tool(
        &self,
        tool_name: &str,
        catalog: &[Tool],
        active: &mut HashSet<String>,
    ) -> bool {
        maybe_activate_requested_deferred_tool(tool_name, catalog, active)
    }

    async fn execute_code_execution_tool(
        &self,
        input: &Value,
        workspace: &std::path::Path,
    ) -> Result<ToolResult, ToolError> {
        execute_code_execution_tool(input, workspace).await
    }

    fn execute_tool_search(
        &self,
        tool_name: &str,
        input: &Value,
        catalog: &[Tool],
        active: &mut HashSet<String>,
    ) -> Result<ToolResult, ToolError> {
        execute_tool_search(tool_name, input, catalog, active)
    }

    fn reset_scratchpad_step(&mut self) {
        self.scratchpad_step.reset();
    }

    async fn refresh_system_prompt(&mut self, mode: TurnLoopMode) {
        Engine::refresh_system_prompt(self, turn_loop_to_app_mode(mode));
    }

    async fn add_session_message(&mut self, message: Message) {
        Engine::add_session_message(self, message).await;
    }

    async fn emit_session_updated(&mut self) {
        Engine::emit_session_updated(self).await;
    }

    async fn run_auto_compaction(&mut self, client: &dyn LlmClient) {
        let compaction_pins = self
            .session
            .working_set
            .pinned_message_indices(&self.session.messages, &self.session.workspace);
        let mut compaction_paths = self.session.working_set.top_paths(24);
        scratchpad_flow::extend_compaction_paths(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
            &mut compaction_paths,
        );

        if !self.config.compaction.enabled
            || !should_compact(
                &self.session.messages,
                &self.config.compaction,
                Some(&self.session.workspace),
                Some(&compaction_pins),
                Some(&compaction_paths),
            )
        {
            return;
        }

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
                    let _ = self.tx_event.send(Event::status(status)).await;
                } else {
                    let message = "Auto-compaction skipped: empty result".to_string();
                    Engine::emit_compaction_failed(self, compaction_id.clone(), true, message.clone())
                        .await;
                    let _ = self.tx_event.send(Event::status(message)).await;
                }
            }
            Err(err) => {
                let message = format!("Auto-compaction failed: {err}");
                Engine::emit_compaction_failed(self, compaction_id, true, message.clone())
                    .await;
                let _ = self.tx_event.send(Event::status(message)).await;
            }
        }
    }

    fn estimated_input_tokens(&self) -> usize {
        estimate_input_tokens_conservative(
            &self.session.messages,
            self.session.system_prompt.as_ref(),
        )
    }

    async fn flush_pending_lsp_diagnostics(&mut self) {
        Engine::flush_pending_lsp_diagnostics(self).await;
    }

    async fn layered_context_checkpoint(&mut self) {
        Engine::layered_context_checkpoint(self).await;
    }

    fn decorate_auth_error_message(&self, message: String) -> String {
        Engine::decorate_auth_error_message(self, message)
    }

    async fn recover_context_overflow(
        &mut self,
        client: &dyn LlmClient,
        reason: &str,
        max_output_tokens: u32,
    ) -> bool {
        Engine::recover_context_overflow(self, client, reason, max_output_tokens).await
    }

    async fn run_capacity_pre_request_checkpoint(
        &mut self,
        turn: &TurnContext,
        client: Option<&dyn LlmClient>,
        mode: TurnLoopMode,
    ) -> bool {
        Engine::run_capacity_pre_request_checkpoint(self, turn, client, turn_loop_to_app_mode(mode))
            .await
    }

    async fn run_capacity_error_escalation_checkpoint(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        step_error_count: usize,
        consecutive_tool_error_steps: u32,
        error_categories: &[deepseek_core::error_taxonomy::ErrorCategory],
    ) -> bool {
        Engine::run_capacity_error_escalation_checkpoint(
            self,
            turn,
            turn_loop_to_app_mode(mode),
            step_error_count,
            consecutive_tool_error_steps,
            error_categories,
        )
        .await
    }

    async fn run_post_edit_lsp_hook(&mut self, tool_name: &str, tool_input: &Value) {
        Engine::run_post_edit_lsp_hook(self, tool_name, tool_input).await;
    }

    fn record_scratchpad_tool_outcome(&mut self, tool_name: &str, success: bool) {
        scratchpad_flow::record_tool_outcome(&mut self.scratchpad_step, tool_name, success);
    }

    async fn maybe_inject_scratchpad_summary(&mut self) -> bool {
        if self.scratchpad_summary_injected_this_turn {
            return false;
        }
        let Some(summary_msg) = scratchpad_flow::maybe_summary_before_final_answer(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
            &self.config.scratchpad,
        ) else {
            return false;
        };
        Engine::add_session_message(self, summary_msg).await;
        self.scratchpad_summary_injected_this_turn = true;
        true
    }

    async fn maybe_inject_scratchpad_reminder(&mut self) {
        if let Some(reminder) = scratchpad_flow::build_readonly_reminder_message(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
            &self.config.scratchpad,
            &self.scratchpad_step,
        ) {
            Engine::add_session_message(self, reminder).await;
        }
    }

    async fn handle_no_tool_uses(
        &mut self,
        turn: &mut TurnContext,
        pending_steers: &mut Vec<String>,
        current_text_visible: &str,
        has_sendable_assistant_content: bool,
    ) -> TurnLoopControl {
        if self.maybe_inject_scratchpad_summary().await && !pending_steers.is_empty() {
            for steer in pending_steers.drain(..) {
                self.session
                    .working_set
                    .observe_user_message(&steer, &self.session.workspace);
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
                self.session
                    .working_set
                    .observe_user_message(&steer, &self.session.workspace);
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
        while let Ok(c) = self.rx_subagent_completion.try_recv() {
            completions.push(c);
        }
        if completions.is_empty() {
            let running = {
                let mgr = self.subagent_manager.read().await;
                mgr.running_count()
            };
            if running > 0 {
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "Waiting on {running} sub-agent(s) to complete..."
                    )))
                    .await;
                tokio::select! {
                    biased;
                    () = self.cancel_token.cancelled() => {
                        let _ = self
                            .tx_event
                            .send(Event::status(
                                "Request cancelled while waiting for sub-agents",
                            ))
                            .await;
                        return TurnLoopControl::Return(TurnOutcomeStatus::Interrupted, None);
                    }
                    Some(c) = self.rx_subagent_completion.recv() => {
                        completions.push(c);
                        while let Ok(extra) = self.rx_subagent_completion.try_recv() {
                            completions.push(extra);
                        }
                    }
                    Some(steer) = self.rx_steer.recv() => {
                        let trimmed = steer.trim().to_string();
                        if !trimmed.is_empty() {
                            self.session
                                .working_set
                                .observe_user_message(&trimmed, &self.session.workspace);
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
            }
        }
        if !completions.is_empty() {
            let count = completions.len();
            for c in completions {
                self.session
                    .working_set
                    .observe_user_message(&c.payload, &self.session.workspace);
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

        TurnLoopControl::Break
    }

    fn pre_tool_snapshot(&self, workspace: &std::path::Path, tool_id: &str) {
        pre_tool_snapshot(workspace, tool_id);
    }

    async fn run_streaming_phase(
        &mut self,
        turn: &mut TurnContext,
        client: &dyn LlmClient,
        mode: TurnLoopMode,
        tool_catalog: &[Tool],
        active_tool_names: &HashSet<String>,
        force_update_plan_first: bool,
        stream_retry_attempts: &mut u32,
        context_recovery_attempts: &mut u8,
        turn_error: &mut Option<String>,
    ) -> TurnLoopStreamingPhaseOutcome {
        super::streaming_phase::run_streaming_phase(
            self,
            turn,
            client,
            mode,
            tool_catalog,
            active_tool_names,
            force_update_plan_first,
            stream_retry_attempts,
            context_recovery_attempts,
            turn_error,
        )
        .await
    }

    async fn run_tool_execution_phase(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        tool_uses: &mut [ToolUseState],
        tool_catalog: &[Tool],
        active_tool_names: &mut HashSet<String>,
        loop_guard: &mut LoopGuard,
        consecutive_tool_error_steps: u32,
        tool_registry: Option<&Self::ToolRegistry>,
    ) -> TurnLoopToolPhaseOutcome {
        super::tool_phase::run_tool_execution_phase(
            self,
            turn,
            mode,
            tool_uses,
            tool_catalog,
            active_tool_names,
            loop_guard,
            consecutive_tool_error_steps,
            tool_registry,
        )
        .await
    }

    async fn run_capacity_post_tool_checkpoint(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        tool_registry: Option<&Self::ToolRegistry>,
        tool_exec_lock: Arc<RwLock<()>>,
        mcp_pool: Option<Arc<AsyncMutex<Self::McpPool>>>,
        step_error_count: usize,
        consecutive_tool_error_steps: u32,
    ) -> bool {
        Engine::run_capacity_post_tool_checkpoint(
            self,
            turn,
            turn_loop_to_app_mode(mode),
            tool_registry,
            tool_exec_lock,
            mcp_pool,
            step_error_count,
            consecutive_tool_error_steps,
        )
        .await
    }
}

#[must_use]
pub(super) fn app_mode_to_turn_loop(mode: AppMode) -> TurnLoopMode {
    match mode {
        AppMode::Agent => TurnLoopMode::Agent,
        AppMode::Yolo => TurnLoopMode::Yolo,
        AppMode::Plan => TurnLoopMode::Plan,
    }
}

#[must_use]
pub(super) fn turn_loop_to_app_mode(mode: TurnLoopMode) -> AppMode {
    match mode {
        TurnLoopMode::Agent => AppMode::Agent,
        TurnLoopMode::Yolo => AppMode::Yolo,
        TurnLoopMode::Plan => AppMode::Plan,
    }
}
