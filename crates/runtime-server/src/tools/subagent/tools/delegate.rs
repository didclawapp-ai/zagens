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

use super::super::constants::DEFAULT_RESULT_TIMEOUT_MS;
use super::super::deprecation::wrap_with_deprecation_notice;
use super::super::executor::wait_for_result;
use super::super::factory::SharedSubAgentManager;
use super::super::parse::parse_spawn_request;
use super::super::registry::summarize_subagent_result;
use super::super::runtime::SubAgentRuntime;
use super::spawn::AgentSpawnTool;
use super::super::types::SubAgentSpawnOptions;

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
        json!({
            "type": "object",
            "properties": {
                "agent_name": {
                    "type": "string",
                    "description": "Name/type alias for the agent (general, explore, plan, review, implementer, verifier, worker, explorer, awaiter, builder, validator, tester)"
                },
                "type": {
                    "type": "string",
                    "description": "Alias for agent_name"
                },
                "agent_type": {
                    "type": "string",
                    "description": "Alias for agent_name"
                },
                "role": {
                    "type": "string",
                    "description": "Role alias: worker, explorer, awaiter, default"
                },
                "agent_role": {
                    "type": "string",
                    "description": "Alias for role"
                },
                "objective": {
                    "type": "string",
                    "description": "The goal or task description for the agent"
                },
                "prompt": {
                    "type": "string",
                    "description": "Alias for objective"
                },
                "message": {
                    "type": "string",
                    "description": "Alias for objective"
                },
                "items": {
                    "type": "array",
                    "description": "Structured input items (text, mention, skill, local_image, image)",
                    "items": {
                        "type": "object"
                    }
                },
                "allowed_tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Explicit tool allowlist (required for custom type)"
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

