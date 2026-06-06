use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::spec::{ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec};

use super::super::constants::COMPLETED_AGENT_RETENTION;
use super::super::factory::SharedSubAgentManager;

pub struct AgentListTool {
    manager: SharedSubAgentManager,
}

impl AgentListTool {
    /// Create a new list tool.
    #[must_use]
    pub fn new(manager: SharedSubAgentManager) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl ToolSpec for AgentListTool {
    fn name(&self) -> &'static str {
        "agent_list"
    }

    fn description(&self) -> &'static str {
        "List **sub-agents** (`agent_spawn`), not durable Tasks — use `task_list` for TaskManager. \
         Shows status, type, steps, duration. Poll until no `Running` before P2 synthesis when \
         children did parallel audit work. `include_archived=true` includes prior-session agents."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "include_archived": {
                    "type": "boolean",
                    "description": "When true, include agents from prior sessions in the listing. Default false."
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let include_archived = input
            .get("include_archived")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut manager = self.manager.write().await;
        manager.cleanup(COMPLETED_AGENT_RETENTION);
        let results = manager.list_filtered(include_archived);
        ToolResult::json(&results).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}
