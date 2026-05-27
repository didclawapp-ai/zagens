
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::spec::{
    ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
};

use super::super::factory::SharedSubAgentManager;
use super::super::parse::parse_assign_request;

pub struct AgentAssignTool {
    manager: SharedSubAgentManager,
    name: &'static str,
}

impl AgentAssignTool {
    /// Create a new assignment tool.
    #[must_use]
    pub fn new(manager: SharedSubAgentManager, name: &'static str) -> Self {
        Self { manager, name }
    }
}

#[async_trait]
impl ToolSpec for AgentAssignTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Update a sub-agent's assignment (objective, role) and optionally deliver an immediate \
         coordinator note. The update is delivered as a high-priority message when `interrupt` is \
         true (the default). Returns the agent's current snapshot."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "Agent id returned by agent_spawn"
                },
                "id": {
                    "type": "string",
                    "description": "Alias for agent_id"
                },
                "objective": {
                    "type": "string",
                    "description": "Updated assignment objective"
                },
                "role": {
                    "type": "string",
                    "description": "Updated role alias: worker, explorer, awaiter, default"
                },
                "agent_role": {
                    "type": "string",
                    "description": "Alias for role"
                },
                "message": {
                    "type": "string",
                    "description": "Optional coordinator note to send to the agent"
                },
                "input": {
                    "type": "string",
                    "description": "Alias for message"
                },
                "items": {
                    "type": "array",
                    "description": "Structured input items (text, mention, skill, local_image, image)",
                    "items": {
                        "type": "object"
                    }
                },
                "interrupt": {
                    "type": "boolean",
                    "description": "Prioritize this assignment update in the agent inbox (default: true)"
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let request = parse_assign_request(&input)?;
        let mut manager = self.manager.write().await;
        let result = manager
            .assign(
                &request.agent_id,
                request.objective,
                request.role,
                request.message,
                request.interrupt,
            )
            .map_err(|e| ToolError::execution_failed(format!("Failed to assign sub-agent: {e}")))?;

        ToolResult::json(&result).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}


