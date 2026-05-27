use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use deepseek_core::subagent::{SubAgentResult, SubAgentStatus};
use crate::tools::spec::{
    ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, optional_u64,
};

use super::super::constants::*;
use super::super::executor::wait_for_agents;
use super::super::factory::SharedSubAgentManager;
use super::super::parse::{parse_wait_ids, parse_wait_mode};
use super::super::registry::subagent_status_name;

pub struct AgentWaitTool {
    manager: SharedSubAgentManager,
    name: &'static str,
}

impl AgentWaitTool {
    /// Create a new wait tool.
    #[must_use]
    pub fn new(manager: SharedSubAgentManager, name: &'static str) -> Self {
        Self { manager, name }
    }
}

#[async_trait]
impl ToolSpec for AgentWaitTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Wait for one or more sub-agents to reach a terminal status. Use `wait_mode: \"all\"` to block \
         until every listed agent finishes, or `wait_mode: \"any\"` (default) to return as soon as \
         one finishes. When no ids are given, waits on all currently running sub-agents."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Agent IDs to wait on. When omitted, waits on all currently running sub-agents."
                },
                "agent_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Alias for ids"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Single agent ID"
                },
                "id": {
                    "type": "string",
                    "description": "Alias for agent_id"
                },
                "wait_mode": {
                    "type": "string",
                    "description": "Wait behavior: any (default) or all"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Max wait time in milliseconds (default: 30000, clamped to 10000-3600000)"
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let timeout_ms = optional_u64(&input, "timeout_ms", DEFAULT_RESULT_TIMEOUT_MS)
            .clamp(MIN_WAIT_TIMEOUT_MS, MAX_RESULT_TIMEOUT_MS);
        let mut ids = parse_wait_ids(&input);
        if ids.is_empty() {
            let manager = self.manager.read().await;
            ids = manager
                .list()
                .into_iter()
                .filter(|snapshot| snapshot.status == SubAgentStatus::Running)
                .map(|snapshot| snapshot.agent_id)
                .collect();
        }
        let wait_mode = parse_wait_mode(&input)?;

        if ids.is_empty() {
            let empty: Vec<SubAgentResult> = Vec::new();
            let mut result =
                ToolResult::json(&empty).map_err(|e| ToolError::execution_failed(e.to_string()))?;
            result.metadata = Some(json!({
                "wait_mode": wait_mode.as_str(),
                "timed_out": false,
                "status": "Completed",
                "timeout_ms": timeout_ms,
                "waited_ids": [],
                "completed_ids": [],
                "running_ids": [],
                "status_by_id": {}
            }));
            return Ok(result);
        }

        let waited_ids = ids.clone();

        let (snapshots, timed_out) = wait_for_agents(
            &self.manager,
            &ids,
            wait_mode,
            Duration::from_millis(timeout_ms),
        )
        .await?;

        let all_done = snapshots
            .iter()
            .all(|snapshot| snapshot.status != SubAgentStatus::Running);
        let completed_ids = snapshots
            .iter()
            .filter(|snapshot| snapshot.status != SubAgentStatus::Running)
            .map(|snapshot| snapshot.agent_id.clone())
            .collect::<Vec<_>>();
        let running_ids = snapshots
            .iter()
            .filter(|snapshot| snapshot.status == SubAgentStatus::Running)
            .map(|snapshot| snapshot.agent_id.clone())
            .collect::<Vec<_>>();
        let status_by_id = snapshots
            .iter()
            .map(|snapshot| {
                (
                    snapshot.agent_id.clone(),
                    subagent_status_name(&snapshot.status).to_string(),
                )
            })
            .collect::<HashMap<_, _>>();

        let mut result =
            ToolResult::json(&snapshots).map_err(|e| ToolError::execution_failed(e.to_string()))?;
        result.metadata = Some(json!({
            "wait_mode": wait_mode.as_str(),
            "timed_out": timed_out,
            "status": if timed_out { "TimedOut" } else if all_done { "Completed" } else { "Partial" },
            "timeout_ms": timeout_ms,
            "waited_ids": waited_ids,
            "completed_ids": completed_ids,
            "running_ids": running_ids,
            "status_by_id": status_by_id
        }));
        Ok(result)
    }
}


