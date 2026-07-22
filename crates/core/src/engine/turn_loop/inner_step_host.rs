//! Inner-step IO surface (`streaming_phase` + `tool_phase`).
//!
//! v3 routes through `EffectInterpreter`; core fallback calls these phases directly.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use zagens_tools::{ToolError, ToolResult};

use tokio::sync::{Mutex as AsyncMutex, RwLock};

use crate::chat::{MessageRequest, Tool};
use crate::engine::kernel_turn_host::KernelTurnHost;
use crate::engine::streaming::ToolUseState;
use crate::turn::{TurnContext, TurnLoopMode};

use super::control::TurnLoopControl;
use super::exec::{ToolExecOutcome, ToolExecutionPlan, ToolPlanApprovalMeta};
use super::host::{CompilerRequestContext, TurnLoopToolRegistry};
use super::turn_loop_session_host::TurnLoopSessionHost;

/// Inner-step host port for streaming/tool execution (v3 effect interpreter + core fallback).
#[async_trait]
pub trait InnerStepHost:
    TurnLoopSessionHost + KernelTurnHost<V3ToolRegistry = Self::ToolRegistry>
{
    type ToolRegistry: TurnLoopToolRegistry;
    type McpPool: crate::engine::hosts::McpHost;

    fn prepare_tool_catalog(&self, catalog: &mut Vec<Tool>);

    fn initial_active_tool_names(&self, catalog: &[Tool]) -> HashSet<String>;

    fn active_tools_for_step(
        &self,
        catalog: &[Tool],
        active: &HashSet<String>,
        force_update_plan_first: bool,
    ) -> Vec<Tool>;

    fn is_mcp_tool_name(&self, name: &str) -> bool;

    fn maybe_activate_deferred_tool(
        &self,
        tool_name: &str,
        catalog: &[Tool],
        active: &mut HashSet<String>,
    ) -> bool;

    async fn execute_code_execution_tool(
        &self,
        input: &Value,
        workspace: &Path,
    ) -> Result<ToolResult, ToolError>;

    fn execute_tool_search(
        &self,
        tool_name: &str,
        input: &Value,
        catalog: &[Tool],
        active: &mut HashSet<String>,
    ) -> Result<ToolResult, ToolError>;

    fn decorate_auth_error_message(&self, message: String) -> String;

    fn effective_reasoning_effort_for_request(&mut self) -> Option<String>;

    fn parse_streaming_tool_input(&self, buffer: &str) -> Option<Value>;

    fn final_streaming_tool_input(&self, state: &ToolUseState) -> Value;

    async fn ensure_mcp_pool_for_tools(
        &mut self,
        tool_uses: &[ToolUseState],
    ) -> Option<Arc<AsyncMutex<Self::McpPool>>>;

    fn resolve_hallucinated_tool_name(
        &self,
        name: &str,
        catalog: &[Tool],
        registry: Option<&Self::ToolRegistry>,
    ) -> Option<String>;

    fn tool_plan_approval_meta(
        &self,
        tool_name: &str,
        tool_input: &Value,
        registry: Option<&Self::ToolRegistry>,
    ) -> ToolPlanApprovalMeta;

    fn model_request_fingerprint(
        &self,
        _request: &MessageRequest,
    ) -> Option<crate::engine::RequestFingerprint> {
        None
    }

    fn compiler_request_context(
        &mut self,
        active_tools: Option<&[Tool]>,
    ) -> Option<CompilerRequestContext> {
        let _ = active_tools;
        None
    }

    /// Push live context snapshot + Explorer breakdown (runtime override, P2-1).
    async fn push_live_context_panel_events(&mut self) {}

    #[allow(clippy::too_many_arguments)]
    async fn execute_tool_plans(
        &mut self,
        mode: TurnLoopMode,
        plans: Vec<ToolExecutionPlan>,
        tool_catalog: &[Tool],
        active_tool_names: &mut HashSet<String>,
        tool_registry: Option<&Self::ToolRegistry>,
        mcp_pool: Option<Arc<AsyncMutex<Self::McpPool>>>,
        tool_exec_lock: Arc<RwLock<()>>,
    ) -> Vec<ToolExecOutcome>;

    async fn flush_pending_lsp_diagnostics(&mut self);

    async fn run_post_edit_lsp_hook(&mut self, tool_name: &str, tool_input: &Value);

    fn record_scratchpad_tool_outcome(&mut self, tool_name: &str, success: bool);

    async fn record_long_horizon_tool_outcome(
        &mut self,
        _tool_name: &str,
        _tool_input: &Value,
        _result: &str,
        _success: bool,
        _tool_use_id: &str,
    ) {
    }

    fn take_long_horizon_tool_suffix(&mut self) -> Option<String> {
        None
    }

    fn on_audit_scratchpad_bind_success(
        &mut self,
        _mode: TurnLoopMode,
        _tool_name: &str,
        _catalog: &mut [Tool],
        _active: &mut HashSet<String>,
    ) {
    }

    async fn handle_no_tool_uses(
        &mut self,
        turn: &mut TurnContext,
        pending_steers: &mut Vec<String>,
        current_text_visible: &str,
        has_sendable_assistant_content: bool,
    ) -> TurnLoopControl;

    fn pre_tool_snapshot(&self, workspace: &Path, tool_id: &str);

    #[allow(clippy::too_many_arguments)]
    async fn run_capacity_post_tool_checkpoint(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        tool_registry: Option<&Self::ToolRegistry>,
        tool_exec_lock: Arc<RwLock<()>>,
        mcp_pool: Option<Arc<AsyncMutex<Self::McpPool>>>,
        step_error_count: usize,
        consecutive_tool_error_steps: u32,
    ) -> bool;
}

#[cfg(test)]
mod tests {
    const INNER_STEP_METHOD_BASELINE: usize = 26;

    #[test]
    fn inner_step_host_method_baseline() {
        let methods = [
            "prepare_tool_catalog",
            "initial_active_tool_names",
            "active_tools_for_step",
            "is_mcp_tool_name",
            "maybe_activate_deferred_tool",
            "execute_code_execution_tool",
            "execute_tool_search",
            "decorate_auth_error_message",
            "effective_reasoning_effort_for_request",
            "parse_streaming_tool_input",
            "final_streaming_tool_input",
            "ensure_mcp_pool_for_tools",
            "resolve_hallucinated_tool_name",
            "tool_plan_approval_meta",
            "model_request_fingerprint",
            "compiler_request_context",
            "execute_tool_plans",
            "flush_pending_lsp_diagnostics",
            "run_post_edit_lsp_hook",
            "record_scratchpad_tool_outcome",
            "record_long_horizon_tool_outcome",
            "take_long_horizon_tool_suffix",
            "on_audit_scratchpad_bind_success",
            "handle_no_tool_uses",
            "pre_tool_snapshot",
            "run_capacity_post_tool_checkpoint",
        ];
        assert_eq!(methods.len(), INNER_STEP_METHOD_BASELINE);
    }
}
