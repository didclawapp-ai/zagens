use std::collections::HashMap;
use std::time::Duration;

use crate::tools::subagent_inputs::agent_wait_input_schema;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::spec::{ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec};
use zagens_core::subagent::{SubAgentResult, SubAgentStatus};

use super::super::constants::*;
use super::super::executor::wait_for_agents;
use super::super::factory::SharedSubAgentManager;
use super::super::parse::{parse_wait_ids, parse_wait_mode};
use super::super::registry::subagent_status_name;
use super::super::wait_timeout::wait_progress_metadata;

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
        agent_wait_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let explicit_timeout_ms = input.get("timeout_ms").and_then(|v| v.as_u64());
        let mut ids = parse_wait_ids(&input);
        if ids.is_empty() {
            let mut manager = self.manager.write().await;
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
                "wait_canceled": false,
                "status": "Completed",
                "timeout_ms": explicit_timeout_ms.unwrap_or(DEFAULT_RESULT_TIMEOUT_MS),
                "waited_ids": [],
                "completed_ids": [],
                "running_ids": [],
                "status_by_id": {}
            }));
            return Ok(result);
        }

        let waited_ids = ids.clone();

        let timeout_ms = if let Some(ms) = explicit_timeout_ms {
            ms.clamp(MIN_WAIT_TIMEOUT_MS, MAX_RESULT_TIMEOUT_MS)
        } else {
            let mut manager = self.manager.write().await;
            manager
                .adaptive_wait_timeout_ms_for_ids(&ids)
                .map_err(|e| ToolError::execution_failed(e.to_string()))?
        };

        let (snapshots, timed_out, wait_canceled) = wait_for_agents(
            &self.manager,
            &ids,
            wait_mode,
            Duration::from_millis(timeout_ms),
            context.cancel_token.as_ref(),
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
        let status = if wait_canceled {
            "Canceled"
        } else if timed_out {
            "TimedOut"
        } else if all_done {
            "Completed"
        } else {
            "Partial"
        };
        let mut metadata = json!({
            "wait_mode": wait_mode.as_str(),
            "timed_out": timed_out,
            "wait_canceled": wait_canceled,
            "status": status,
            "timeout_ms": timeout_ms,
            "waited_ids": waited_ids,
            "completed_ids": completed_ids,
            "running_ids": running_ids,
            "status_by_id": status_by_id
        });
        if timed_out
            && !wait_canceled
            && snapshots.len() == 1
            && let Some(progress) = metadata.as_object_mut()
            && let Some(progress_obj) = wait_progress_metadata(&snapshots[0]).as_object()
        {
            for (key, value) in progress_obj {
                progress.insert(key.clone(), value.clone());
            }
        }
        result.metadata = Some(metadata);
        Ok(result)
    }
}
