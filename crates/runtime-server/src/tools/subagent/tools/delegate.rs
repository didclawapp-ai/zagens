use crate::tools::subagent_inputs::delegate_to_agent_input_schema;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
};

use super::super::deprecation::wrap_with_deprecation_notice;
use super::super::factory::SharedSubAgentManager;
use super::super::runtime::SubAgentRuntime;
use super::spawn::AgentSpawnTool;

pub struct DelegateToAgentTool {
    manager: SharedSubAgentManager,
    runtime: SubAgentRuntime,
}

impl DelegateToAgentTool {
    /// Create a new delegation tool.
    #[must_use]
    pub fn new(manager: SharedSubAgentManager, runtime: SubAgentRuntime) -> Self {
        Self { manager, runtime }
    }
}

#[async_trait]
impl ToolSpec for DelegateToAgentTool {
    fn name(&self) -> &'static str {
        "delegate_to_agent"
    }

    fn description(&self) -> &'static str {
        "Delegate a task to a specialized sub-agent. This is an alias for agent_spawn — same schema, \
         same behavior. Use `type` (or `agent_name`, `agent_type`) to pick the agent flavor."
    }

    fn input_schema(&self) -> Value {
        delegate_to_agent_input_schema()
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

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let spawn_tool = AgentSpawnTool::new(self.manager.clone(), self.runtime.clone());
        let result = spawn_tool.execute(input, context).await?;
        Ok(wrap_with_deprecation_notice(
            result,
            "delegate_to_agent",
            "agent_spawn",
        ))
    }
}
