//! Import structured sub-agent output into audit scratchpad.

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::scratchpad::{
    ScratchpadStore, import_agent_findings, resolve_run_id, validate_agent_run_binding,
};
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_str, required_str,
};

use crate::tools::subagent::{SharedSubAgentManager, wait_for_result};
use std::time::Duration;

const DEFAULT_IMPORT_TIMEOUT_MS: u64 = 30_000;

pub struct ScratchpadImportAgentTool {
    manager: SharedSubAgentManager,
}

impl ScratchpadImportAgentTool {
    #[must_use]
    pub fn new(manager: SharedSubAgentManager) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl ToolSpec for ScratchpadImportAgentTool {
    fn name(&self) -> &'static str {
        "scratchpad_import_agent"
    }

    fn description(&self) -> &'static str {
        "Import a completed Explore/Review sub-agent's structured_findings (or structured_verdict) \
         into notes.jsonl as status=open rows. Parent must scratchpad_verify_note after read_file/grep_files."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "run_id": {
                    "type": "string",
                    "description": "Scratchpad run id (defaults to active thread/task scratchpad)"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Sub-agent id from agent_spawn"
                },
                "area_id": {
                    "type": "string",
                    "description": "Inventory area_id to import under. Use when the child emitted a mismatched area_id; must exist in inventory. When omitted, runtime also tries structured_findings.area_path against inventory paths."
                },
                "block": {
                    "type": "boolean",
                    "description": "Wait for agent completion before import (default true)"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Max wait when block=true (default 30000, max 3600000)"
                }
            },
            "required": ["agent_id"],
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::WritesFiles]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let agent_id = required_str(&input, "agent_id")?;
        let run_id = resolve_run_id(context, optional_str(&input, "run_id"))?;
        let area_override = optional_str(&input, "area_id");
        let block = input.get("block").and_then(|v| v.as_bool()).unwrap_or(true);
        let timeout_ms = input
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_IMPORT_TIMEOUT_MS)
            .clamp(1000, 3_600_000);

        let result = if block {
            let (_, timed_out) =
                wait_for_result(&self.manager, agent_id, Duration::from_millis(timeout_ms)).await?;
            if timed_out {
                let still_running = self
                    .manager
                    .write()
                    .await
                    .get_result(agent_id)
                    .map(|r| r.status == zagens_core::subagent::SubAgentStatus::Running)
                    .unwrap_or(false);
                if still_running {
                    return Err(ToolError::execution_failed(format!(
                        "agent '{agent_id}' still running after {timeout_ms}ms"
                    )));
                }
            }
            // After wait, enrich from blackboard + prose (same as agent_result).
            self.manager
                .write()
                .await
                .get_result_with_fallback(agent_id, &context.workspace)
                .map_err(|e| ToolError::execution_failed(e.to_string()))?
        } else {
            self.manager
                .write()
                .await
                .get_result_with_fallback(agent_id, &context.workspace)
                .map_err(|e| ToolError::execution_failed(e.to_string()))?
        };

        let store = ScratchpadStore::open(context, &run_id)?;
        let bound_run = self
            .manager
            .write()
            .await
            .agent_scratchpad_run_id(agent_id)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        validate_agent_run_binding(bound_run.as_deref(), &run_id, agent_id)?;
        let notes = import_agent_findings(&store, &result, area_override)?;
        let out = json!({
            "run_id": run_id,
            "agent_id": agent_id,
            "imported": notes.iter().map(|n| json!({
                "id": n.id,
                "kind": n.kind,
                "status": n.status,
                "severity": n.severity,
            })).collect::<Vec<_>>(),
            "count": notes.len(),
        });
        Ok(ToolResult::success(
            serde_json::to_string_pretty(&out).unwrap_or_default(),
        ))
    }
}
