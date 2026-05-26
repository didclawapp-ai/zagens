use std::time::Duration;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use deepseek_core::subagent::{SubAgentAssignment, SubAgentResult, SubAgentStatus, SubAgentType};
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool, optional_u64, required_str,
};

use super::super::constants::*;
use super::super::deprecation::wrap_with_deprecation_notice;
use super::super::executor::wait_for_result;
use super::super::factory::SharedSubAgentManager;
use super::super::registry::summarize_subagent_result;
use super::super::runtime::SubAgentRuntime;

pub struct AgentResultTool {
    manager: SharedSubAgentManager,
}

impl AgentResultTool {
    /// Create a new result tool.
    #[must_use]
    pub fn new(manager: SharedSubAgentManager) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl ToolSpec for AgentResultTool {
    fn name(&self) -> &'static str {
        "agent_result"
    }

    fn description(&self) -> &'static str {
        "Get status or final output for a **sub-agent** (`agent_spawn` id). Use `block: true` and \
         a large `timeout_ms` for long audits. For durable **Tasks** (`task_create`), use \
         `task_read` instead — different object, different namespace."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "ID returned by agent_spawn"
                },
                "id": {
                    "type": "string",
                    "description": "Alias for agent_id"
                },
                "block": {
                    "type": "boolean",
                    "description": "Wait for completion (default: false)"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Max wait time in milliseconds (default: 30000, clamped to 1000-3600000)"
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let agent_id = input
            .get("agent_id")
            .or_else(|| input.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::missing_field("agent_id"))?;
        let block = optional_bool(&input, "block", false);
        let timeout_ms = optional_u64(&input, "timeout_ms", DEFAULT_RESULT_TIMEOUT_MS)
            .clamp(1000, MAX_RESULT_TIMEOUT_MS);

        let (result, timed_out) = if block {
            wait_for_result(&self.manager, agent_id, Duration::from_millis(timeout_ms)).await?
        } else {
            let manager = self.manager.read().await;
            (
                manager
                    .get_result(agent_id)
                    .map_err(|e| ToolError::execution_failed(e.to_string()))?,
                false,
            )
        };

        let mut tool_result =
            ToolResult::json(&result).map_err(|e| ToolError::execution_failed(e.to_string()))?;
        if timed_out {
            tool_result.metadata = Some(json!({
                "status": "TimedOut",
                "timed_out": true,
                "timeout_ms": timeout_ms
            }));
        } else if result.status == SubAgentStatus::Running {
            tool_result.metadata = Some(json!({ "status": "Running" }));
        }
        Ok(tool_result)
    }
}


