
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::spec::{
    ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool,
};

use super::super::deprecation::wrap_with_deprecation_notice;
use super::super::factory::SharedSubAgentManager;
use super::super::parse::parse_text_or_items;

pub struct AgentSendInputTool {
    manager: SharedSubAgentManager,
    name: &'static str,
}

impl AgentSendInputTool {
    /// Create a new send-input tool.
    #[must_use]
    pub fn new(manager: SharedSubAgentManager, name: &'static str) -> Self {
        Self { manager, name }
    }
}

#[async_trait]
impl ToolSpec for AgentSendInputTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Send input to a running sub-agent. Returns the agent's current snapshot after delivery."
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
                "message": {
                    "type": "string",
                    "description": "Message to deliver to the agent"
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
                    "description": "Prioritize this message over pending inputs"
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let agent_id = input
            .get("agent_id")
            .or_else(|| input.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::missing_field("agent_id"))?;
        let message = parse_text_or_items(&input, &["message", "input"], "items", "message")?;
        let interrupt = optional_bool(&input, "interrupt", false);

        let mut manager = self.manager.write().await;
        manager
            .send_input(agent_id, message, interrupt)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        let snapshot = manager
            .get_result(agent_id)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;

        let tool_result =
            ToolResult::json(&snapshot).map_err(|e| ToolError::execution_failed(e.to_string()))?;
        // Annotate the alias name "send_input" with a deprecation notice;
        // the canonical name "agent_send_input" passes through unchanged.
        if self.name == "send_input" {
            Ok(wrap_with_deprecation_notice(
                tool_result,
                "send_input",
                "agent_send_input",
            ))
        } else {
            Ok(tool_result)
        }
    }
}


