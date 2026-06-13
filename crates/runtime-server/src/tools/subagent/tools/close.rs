use crate::tools::subagent_inputs::agent_close_input_schema;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
};

use super::super::deprecation::wrap_with_deprecation_notice;
use super::super::factory::SharedSubAgentManager;

pub struct AgentCloseTool {
    manager: SharedSubAgentManager,
}

impl AgentCloseTool {
    /// Create a new close tool.
    #[must_use]
    pub fn new(manager: SharedSubAgentManager) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl ToolSpec for AgentCloseTool {
    fn name(&self) -> &'static str {
        "close_agent"
    }

    fn description(&self) -> &'static str {
        "Close a running sub-agent. Alias for agent_cancel."
    }

    fn input_schema(&self) -> Value {
        agent_close_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::ExecutesCode,
            ToolCapability::RequiresApproval,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let agent_id = input
            .get("id")
            .or_else(|| input.get("agent_id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::missing_field("id"))?;
        let mut manager = self.manager.write().await;
        let result = manager
            .cancel(agent_id)
            .map_err(|e| ToolError::execution_failed(format!("Failed to close sub-agent: {e}")))?;
        let tool_result =
            ToolResult::json(&result).map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(wrap_with_deprecation_notice(
            tool_result,
            "close_agent",
            "agent_cancel",
        ))
    }
}
