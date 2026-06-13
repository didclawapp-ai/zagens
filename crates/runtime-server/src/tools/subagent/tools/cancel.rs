use crate::tools::subagent_inputs::agent_cancel_input_schema;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, required_str,
};

use super::super::factory::SharedSubAgentManager;

pub struct AgentCancelTool {
    manager: SharedSubAgentManager,
}

impl AgentCancelTool {
    /// Create a new cancel tool.
    #[must_use]
    pub fn new(manager: SharedSubAgentManager) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl ToolSpec for AgentCancelTool {
    fn name(&self) -> &'static str {
        "agent_cancel"
    }

    fn description(&self) -> &'static str {
        "Cancel a running sub-agent. Returns the final snapshot with the cancelled status."
    }

    fn input_schema(&self) -> Value {
        agent_cancel_input_schema()
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
        let agent_id = required_str(&input, "agent_id")?;
        let mut manager = self.manager.write().await;
        let result = manager
            .cancel(agent_id)
            .map_err(|e| ToolError::execution_failed(format!("Failed to cancel sub-agent: {e}")))?;

        ToolResult::json(&result).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}
