//! Tool dispatch — plan/execute helpers for the per-turn tool batch (P2 PR4).
//!
//! Policy and JSON parsing live in `deepseek-core::engine::dispatch`; this
//! module keeps TUI-only types (`ToolExecutionPlan`, lock guards) and
//! `arg_repair` integration.

use serde_json::Value;

use crate::models::{Tool, ToolCaller};
use crate::tools::spec::{ToolError, ToolResult};
use crate::tui::app::AppMode;

use deepseek_core::engine::dispatch::{self, ToolParallelPlanFlags};
use deepseek_core::engine::streaming::ToolUseState;

pub use deepseek_core::engine::dispatch::{
    caller_allowed_for_tool, caller_type_for_tool_use, format_tool_error,
    mcp_tool_approval_description, mcp_tool_is_parallel_safe, mcp_tool_is_read_only,
    parse_parallel_tool_calls,
};

// === Types (stay in tui until Engine moves to core) ===================

#[allow(dead_code)]
pub(super) struct ToolExecOutcome {
    pub(super) index: usize,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) input: serde_json::Value,
    pub(super) started_at: std::time::Instant,
    pub(super) result: Result<ToolResult, ToolError>,
}

#[derive(Debug, Clone)]
pub(super) struct ToolExecutionPlan {
    pub(super) index: usize,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) input: serde_json::Value,
    pub(super) caller: Option<ToolCaller>,
    pub(super) interactive: bool,
    pub(super) approval_required: bool,
    pub(super) approval_description: String,
    pub(super) supports_parallel: bool,
    pub(super) read_only: bool,
    pub(super) blocked_error: Option<ToolError>,
    pub(super) guard_result: Option<ToolResult>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct ParallelToolResultEntry {
    pub(super) tool_name: String,
    pub(super) success: bool,
    pub(super) content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct ParallelToolResult {
    pub(super) results: Vec<ParallelToolResultEntry>,
}

pub(super) enum ToolExecGuard<'a> {
    Read(#[allow(dead_code)] tokio::sync::RwLockReadGuard<'a, ()>),
    Write(#[allow(dead_code)] tokio::sync::RwLockWriteGuard<'a, ()>),
}

// === TUI arg_repair + core JSON ladder =================================

pub(super) fn parse_tool_input(buffer: &str) -> Option<Value> {
    if let Ok(value) = crate::tools::arg_repair::repair(buffer) {
        return Some(value);
    }
    dispatch::parse_tool_input_json(buffer)
}

/// Like core `final_tool_input`, but runs TUI `arg_repair` on the stream buffer first.
#[must_use]
pub(super) fn final_tool_input(state: &ToolUseState) -> Value {
    if !state.input_buffer.trim().is_empty()
        && let Some(parsed) = parse_tool_input(&state.input_buffer)
    {
        return parsed;
    }
    state.input.clone()
}

pub(super) fn should_parallelize_tool_batch(plans: &[ToolExecutionPlan]) -> bool {
    let flags: Vec<ToolParallelPlanFlags> = plans
        .iter()
        .map(|plan| ToolParallelPlanFlags {
            read_only: plan.read_only,
            supports_parallel: plan.supports_parallel,
            approval_required: plan.approval_required,
            interactive: plan.interactive,
        })
        .collect();
    dispatch::should_parallelize_tool_batch(&flags)
}

pub(super) fn should_stop_after_plan_tool(
    mode: AppMode,
    tool_name: &str,
    result: &Result<ToolResult, ToolError>,
) -> bool {
    dispatch::should_stop_after_plan_tool(mode == AppMode::Plan, tool_name, result)
}

pub(super) fn should_force_update_plan_first(mode: AppMode, content: &str) -> bool {
    dispatch::should_force_update_plan_first(mode == AppMode::Plan, content)
}
