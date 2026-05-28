use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use deepseek_core::subagent::SubAgentStatus;
use crate::tools::spec::{
    ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool,
};

use super::super::constants::*;
use super::super::executor::wait_for_result;
use super::super::factory::SharedSubAgentManager;
use super::super::wait_timeout::wait_progress_metadata;

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
                    "description": "Max wait time in milliseconds when block=true. When omitted, defaults adaptively from the agent's step_timeout_ms and remaining steps (clamped 1000-3600000). Explicit values override."
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let agent_id = input
            .get("agent_id")
            .or_else(|| input.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::missing_field("agent_id"))?;
        let block = optional_bool(&input, "block", false);
        let explicit_timeout_ms = input.get("timeout_ms").and_then(|v| v.as_u64());

        let (result, timed_out) = if block {
            let timeout_ms = if let Some(ms) = explicit_timeout_ms {
                ms.clamp(1000, MAX_RESULT_TIMEOUT_MS)
            } else {
                let mut manager = self.manager.write().await;
                manager
                    .adaptive_wait_timeout_ms_for(agent_id)
                    .map_err(|e| ToolError::execution_failed(e.to_string()))?
            };
            wait_for_result(&self.manager, agent_id, Duration::from_millis(timeout_ms)).await?
        } else {
            let mut manager = self.manager.write().await;
            (
                manager
                    .get_result_with_fallback(agent_id, &context.workspace)
                    .map_err(|e| ToolError::execution_failed(e.to_string()))?,
                false,
            )
        };

        let mut tool_result =
            ToolResult::json(&result).map_err(|e| ToolError::execution_failed(e.to_string()))?;
        if timed_out {
            let mut metadata = json!({
                "status": "TimedOut",
                "timed_out": true,
            });
            if let Some(progress) = metadata.as_object_mut() {
                if let Some(progress_obj) = wait_progress_metadata(&result).as_object() {
                    for (key, value) in progress_obj {
                        progress.insert(key.clone(), value.clone());
                    }
                }
            }
            tool_result.metadata = Some(metadata);
        } else if result.status == SubAgentStatus::Running {
            tool_result.metadata = Some(json!({ "status": "Running" }));
        }
        Ok(tool_result)
    }
}


