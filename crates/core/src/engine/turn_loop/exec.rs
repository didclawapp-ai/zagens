//! Tool-batch execution types for the turn loop (P2 PR4).

use std::time::Instant;

use deepseek_tools::{ToolError, ToolResult};
use serde_json::Value;

use crate::chat::ToolCaller;

#[derive(Debug, Clone)]
pub struct ToolExecOutcome {
    pub index: usize,
    pub id: String,
    pub name: String,
    pub input: Value,
    pub started_at: Instant,
    pub result: Result<ToolResult, ToolError>,
}

#[derive(Debug, Clone)]
pub struct ToolExecutionPlan {
    pub index: usize,
    pub id: String,
    pub name: String,
    pub input: Value,
    pub caller: Option<ToolCaller>,
    pub interactive: bool,
    pub approval_required: bool,
    pub approval_description: String,
    pub supports_parallel: bool,
    pub read_only: bool,
    pub blocked_error: Option<ToolError>,
    pub guard_result: Option<ToolResult>,
}
