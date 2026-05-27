use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
};

use super::super::factory::SharedSubAgentManager;
use super::super::runtime::SubAgentRuntime;

pub struct AgentResumeTool {
    manager: SharedSubAgentManager,
    runtime: SubAgentRuntime,
}

impl AgentResumeTool {
    /// Create a new resume tool.
    #[must_use]
    pub fn new(manager: SharedSubAgentManager, runtime: SubAgentRuntime) -> Self {
        Self { manager, runtime }
    }
}

#[async_trait]
impl ToolSpec for AgentResumeTool {
    fn name(&self) -> &'static str {
        "resume_agent"
    }

    fn description(&self) -> &'static str {
        "Resume a previously closed or completed sub-agent by restarting its assignment."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Agent id to resume"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Alias for id"
                }
            }
        })
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
            .resume(Arc::clone(&self.manager), self.runtime.clone(), agent_id)
            .map_err(|e| ToolError::execution_failed(format!("Failed to resume sub-agent: {e}")))?;
        ToolResult::json(&result).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

