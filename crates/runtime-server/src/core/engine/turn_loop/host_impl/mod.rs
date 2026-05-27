//! `TurnLoopHost` implementation for the TUI `Engine` (P2 PR4 step 2).

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use deepseek_core::chat::{LlmClient, Message, Tool};
use deepseek_core::engine::context::estimate_input_tokens_conservative;
use deepseek_core::engine::turn_loop::control::TurnLoopControl;
use deepseek_core::engine::turn_loop::exec::{
    ToolExecOutcome, ToolExecutionPlan, ToolPlanApprovalMeta,
};
use deepseek_core::engine::turn_loop::{build_edit_file_approval_desc, TurnLoopToolRegistry};
use deepseek_core::engine::tool_catalog::{CODE_EXECUTION_TOOL_NAME, is_tool_search_tool};
use deepseek_core::engine::dispatch::{
    mcp_tool_approval_description, mcp_tool_is_parallel_safe, mcp_tool_is_read_only,
};
use deepseek_core::engine::hosts::McpHost;
use deepseek_core::engine::TurnLoopHost;
use deepseek_core::engine::streaming::ToolUseState;
use deepseek_core::turn::{TurnContext, TurnLoopMode};
use deepseek_tools::{ToolError, ToolResult};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex as AsyncMutex, RwLock};

use deepseek_core::engine::tool_catalog::{
    active_tools_for_step, ensure_advanced_tooling, execute_tool_search, initial_active_tools,
    maybe_activate_requested_deferred_tool,
};
use super::super::tool_catalog::execute_code_execution_tool;
use super::super::scratchpad_flow;
use super::Engine;
use crate::compaction::{compact_messages_safe, should_compact};
use crate::core::events::Event;
use crate::core::turn::pre_tool_snapshot;
use crate::mcp::McpPool;
use crate::agent_surface::AppMode;
use crate::tools::spec::ApprovalRequirement;
use crate::tools::ToolRegistry;
impl TurnLoopToolRegistry for ToolRegistry {}


mod capacity;
mod no_tool_uses;
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
        self.turn_loop_capacity_pre_request(turn, client, mode).await
    }

    async fn run_capacity_error_escalation_checkpoint(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        step_error_count: usize,
        consecutive_tool_error_steps: u32,
        error_categories: &[deepseek_core::error_taxonomy::ErrorCategory],
    ) -> bool {
        self.turn_loop_capacity_error_escalation(
            turn,
            mode,
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
        self.handle_no_tool_uses_turn_loop(
            turn,
            pending_steers,
            current_text_visible,
            has_sendable_assistant_content,
        )
        .await
    }

    fn pre_tool_snapshot(&self, workspace: &std::path::Path, tool_id: &str) {
        pre_tool_snapshot(workspace, tool_id);
    }

    fn effective_reasoning_effort_for_request(&mut self) -> Option<String> {
        deepseek_core::engine::turn_loop::resolve_auto_effort(
            self.session.reasoning_effort.as_deref(),
            &self.session.messages,
            |is_subagent, last_msg| {
                crate::auto_reasoning::select(is_subagent, last_msg)
                    .as_setting()
                    .to_string()
            },
        )
    }

    fn parse_streaming_tool_input(&self, buffer: &str) -> Option<Value> {
        super::super::dispatch::parse_tool_input(buffer)
    }

    fn final_streaming_tool_input(&self, state: &ToolUseState) -> Value {
        super::super::dispatch::final_tool_input(state)
    }

    async fn ensure_mcp_pool_for_tools(
        &mut self,
        tool_uses: &[ToolUseState],
    ) -> Option<Arc<AsyncMutex<McpPool>>> {
        if !tool_uses.iter().any(|tool| McpPool::is_mcp_tool(&tool.name)) {
            return None;
        }
        match self.ensure_mcp_pool().await {
            Ok(pool) => Some(pool),
            Err(err) => {
                let _ = self
                    .tx_event
                    .send(Event::status(err.to_string()))
                    .await;
                None
            }
        }
    }

    fn resolve_hallucinated_tool_name(
        &self,
        name: &str,
        catalog: &[Tool],
        registry: Option<&Self::ToolRegistry>,
    ) -> Option<String> {
        let registry = registry?;
        let canonical = registry.resolve(name)?;
        if catalog.iter().any(|d| d.name == canonical) {
            Some(canonical.to_string())
        } else {
            None
        }
    }

    fn tool_plan_approval_meta(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        registry: Option<&Self::ToolRegistry>,
    ) -> ToolPlanApprovalMeta {
        if McpPool::is_mcp_tool(tool_name) {
            return ToolPlanApprovalMeta {
                read_only: mcp_tool_is_read_only(tool_name),
                supports_parallel: mcp_tool_is_parallel_safe(tool_name),
                approval_required: !mcp_tool_is_read_only(tool_name),
                approval_description: mcp_tool_approval_description(tool_name),
            };
        }
        if let Some(registry) = registry
            && let Some(spec) = registry.get(tool_name)
        {
            return ToolPlanApprovalMeta {
                approval_required: spec.approval_requirement() != ApprovalRequirement::Auto,
                approval_description: if tool_name == "edit_file" {
                    build_edit_file_approval_desc(tool_input)
                } else {
                    spec.description().to_string()
                },
                supports_parallel: spec.supports_parallel(),
                read_only: spec.is_read_only(),
            };
        }
        if tool_name == CODE_EXECUTION_TOOL_NAME {
            return ToolPlanApprovalMeta {
                approval_required: true,
                approval_description:
                    "Run model-provided Python code in local execution sandbox".to_string(),
                supports_parallel: false,
                read_only: false,
            };
        }
        if is_tool_search_tool(tool_name) {
            return ToolPlanApprovalMeta {
                approval_required: false,
                approval_description: "Search tool catalog".to_string(),
                supports_parallel: false,
                read_only: true,
            };
        }
        ToolPlanApprovalMeta {
            approval_required: false,
            approval_description: String::new(),
            supports_parallel: false,
            read_only: false,
        }
    }

    async fn execute_tool_plans(
        &mut self,
        mode: TurnLoopMode,
        plans: Vec<ToolExecutionPlan>,
        tool_catalog: &[Tool],
        active_tool_names: &mut HashSet<String>,
        tool_registry: Option<&Self::ToolRegistry>,
        mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
        tool_exec_lock: Arc<RwLock<()>>,
    ) -> Vec<ToolExecOutcome> {
        super::tool_plans_exec::execute_tool_plans(
            self,
            mode,
            plans,
            tool_catalog,
            active_tool_names,
            tool_registry,
            mcp_pool,
            tool_exec_lock,
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
        self.turn_loop_capacity_post_tool(
            turn,
            mode,
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
pub(crate) fn app_mode_to_turn_loop(mode: AppMode) -> TurnLoopMode {
    match mode {
        AppMode::Agent => TurnLoopMode::Agent,
        AppMode::Yolo => TurnLoopMode::Yolo,
        AppMode::Plan => TurnLoopMode::Plan,
    }
}

#[must_use]
pub(crate) fn turn_loop_to_app_mode(mode: TurnLoopMode) -> AppMode {
    match mode {
        TurnLoopMode::Agent => AppMode::Agent,
        TurnLoopMode::Yolo => AppMode::Yolo,
        TurnLoopMode::Plan => AppMode::Plan,
    }
}

#[cfg(test)]
mod m4_drift_guard {
    use super::*;
    use deepseek_core::engine::dispatch::is_mcp_tool_name;

    /// M4 cross-verify: the tui-side inherent `McpPool::is_mcp_tool`
    /// (in `crates/tui/src/mcp.rs:1498` — frozen per spike §6 M4
    /// "zero changes to mcp.rs body") and the core-side free function
    /// `deepseek_core::engine::dispatch::is_mcp_tool_name` must
    /// produce identical output on every name in a curated set.
    ///
    /// If either definition gains a new prefix / matched literal,
    /// this test fails and forces a mirrored update.
    #[test]
    fn is_mcp_tool_name_matches_tui_mcp_pool() {
        const NAMES: &[&str] = &[
            "mcp_filesystem_read",
            "mcp_filesystem_write",
            "mcp_git_status",
            "mcp_browser_navigate",
            "list_mcp_resources",
            "list_mcp_resource_templates",
            "read_mcp_resource",
            "mcp_",
            "read_file",
            "edit_file",
            "exec_shell",
            "request_user_input",
            "update_plan",
            "tool_search_bm25",
            "",
        ];
        for name in NAMES {
            assert_eq!(
                McpPool::is_mcp_tool(name),
                is_mcp_tool_name(name),
                "drift between tui::mcp::McpPool::is_mcp_tool and \
                 core::engine::dispatch::is_mcp_tool_name on {name:?}"
            );
        }
    }

    /// M4 promoted `TurnLoopMcpPool` to `McpHost`. `McpPool` keeps
    /// satisfying the (deprecated) marker via the blanket impl, so
    /// existing `Self::McpPool: TurnLoopMcpPool` bounds continue to
    /// work. The new `McpHost` default-impl predicates must match
    /// the inherent / free-fn surface.
    #[test]
    fn mcp_pool_satisfies_mcp_host_with_default_impls() {
        fn _accepts<T: McpHost>(_: &T) {}
        let pool: Option<McpPool> = None;
        if let Some(ref p) = pool {
            _accepts(p);
            assert_eq!(
                p.is_mcp_tool("mcp_filesystem_read"),
                McpPool::is_mcp_tool("mcp_filesystem_read")
            );
        }
        // Type-level assertion: McpPool: McpHost.
        const _: fn() = || {
            fn _bound<T: McpHost>() {}
            _bound::<McpPool>();
        };
    }
}
