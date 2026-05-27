//! Audit scratchpad tools (`scratchpad_status`, `scratchpad_append`, …).

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::scratchpad::AreaStatus;
use crate::scratchpad::{ScratchpadStore, display_run_path, resolve_run_id};

fn persist_scratchpad_run(ctx: &ToolContext, run_id: &str) {
    if let Ok(mut guard) = ctx.runtime.wire.scratchpad_run_id.lock() {
        *guard = Some(run_id.to_string());
    }
    if let Some(persist) = &ctx.runtime.wire.persist_scratchpad_run_id {
        persist(run_id.to_string());
    }
}
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, optional_str,
    required_str,
};

fn run_id_property() -> Value {
    json!({
        "type": "string",
        "description": "Scratchpad run directory name. Defaults to active thread_id or task_id when that directory exists."
    })
}

#[derive(Debug, Default)]
pub struct ScratchpadStatusTool;

#[async_trait]
impl ToolSpec for ScratchpadStatusTool {
    fn name(&self) -> &'static str {
        "scratchpad_status"
    }

    fn description(&self) -> &'static str {
        "Return audit scratchpad progress: inventory completion, note counts, resume_area_id, and findings tallies."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "run_id": run_id_property()
            },
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(
        &self,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let run_id = resolve_run_id(context, optional_str(&input, "run_id"))?;
        let store = ScratchpadStore::open(context, &run_id)?;
        persist_scratchpad_run(context, &run_id);
        let status = store.build_status()?;
        Ok(ToolResult::success(
            serde_json::to_string_pretty(&status).unwrap_or_default(),
        ))
    }
}

#[derive(Debug, Default)]
pub struct ScratchpadAppendTool;

#[async_trait]
impl ToolSpec for ScratchpadAppendTool {
    fn name(&self) -> &'static str {
        "scratchpad_append"
    }

    fn description(&self) -> &'static str {
        "Append one validated line to notes.jsonl (auto id, ts). area_id must exist in inventory.json (except kind=meta with area_id=_global)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "run_id": run_id_property(),
                "line": {
                    "type": "object",
                    "description": "One notes.jsonl row (runtime adds id, ts).",
                    "properties": {
                        "area_id": { "type": "string" },
                        "area": { "type": "string" },
                        "kind": { "type": "string", "enum": ["finding", "todo", "cleared", "meta"] },
                        "severity": { "type": "string" },
                        "title": { "type": "string" },
                        "file": { "type": "string" },
                        "line": { "type": "integer" },
                        "line_end": { "type": "integer" },
                        "claim": { "type": "string" },
                        "evidence": { "type": "string" },
                        "status": { "type": "string" },
                        "source": { "type": "string" },
                        "supersedes": { "type": "string" }
                    },
                    "additionalProperties": false
                }
            },
            "required": ["line"],
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::WritesFiles]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(
        &self,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let run_id = resolve_run_id(context, optional_str(&input, "run_id"))?;
        let line = input
            .get("line")
            .cloned()
            .ok_or_else(|| ToolError::missing_field("line"))?;
        let store = ScratchpadStore::open(context, &run_id)?;
        let note = store.append_note(line)?;
        persist_scratchpad_run(context, &run_id);
        let out = json!({
            "id": note.id,
            "path": format!("{}/notes.jsonl", display_run_path(&run_id))
        });
        Ok(ToolResult::success(
            serde_json::to_string_pretty(&out).unwrap_or_default(),
        ))
    }
}

#[derive(Debug, Default)]
pub struct ScratchpadListNotesTool;

#[async_trait]
impl ToolSpec for ScratchpadListNotesTool {
    fn name(&self) -> &'static str {
        "scratchpad_list_notes"
    }

    fn description(&self) -> &'static str {
        "List recent notes.jsonl entries for one area_id (full JSON objects, not summaries)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "run_id": run_id_property(),
                "area_id": {
                    "type": "string",
                    "description": "Inventory area id to filter on"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max notes to return (default 20)",
                    "minimum": 1,
                    "maximum": 100
                }
            },
            "required": ["area_id"],
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(
        &self,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let run_id = resolve_run_id(context, optional_str(&input, "run_id"))?;
        persist_scratchpad_run(context, &run_id);
        let area_id = required_str(&input, "area_id")?;
        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .clamp(1, 100) as usize;
        let store = ScratchpadStore::open(context, &run_id)?;
        let notes = store.list_notes(area_id, limit)?;
        let out = json!({ "area_id": area_id, "notes": notes });
        Ok(ToolResult::success(
            serde_json::to_string_pretty(&out).unwrap_or_default(),
        ))
    }
}

#[derive(Debug, Default)]
pub struct ScratchpadSetAreaTool;

#[async_trait]
impl ToolSpec for ScratchpadSetAreaTool {
    fn name(&self) -> &'static str {
        "scratchpad_set_area"
    }

    fn description(&self) -> &'static str {
        "Update one inventory area status. For status=done, require_min_notes (default 1) must be met — append notes first."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "run_id": run_id_property(),
                "area_id": { "type": "string" },
                "status": {
                    "type": "string",
                    "enum": ["in_progress", "done", "deferred", "pending"]
                },
                "notes": {
                    "type": "string",
                    "description": "Optional human remark on the inventory row (not used for gates)"
                },
                "require_min_notes": {
                    "type": "integer",
                    "description": "When status=done, minimum notes.jsonl lines for this area_id (default 1)",
                    "minimum": 0,
                    "maximum": 50
                }
            },
            "required": ["area_id", "status"],
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::WritesFiles]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(
        &self,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let run_id = resolve_run_id(context, optional_str(&input, "run_id"))?;
        let area_id = required_str(&input, "area_id")?;
        let status_str = required_str(&input, "status")?;
        let status = AreaStatus::from_str(status_str).ok_or_else(|| {
            ToolError::invalid_input(format!(
                "invalid status '{status_str}'; use pending|in_progress|done|deferred"
            ))
        })?;
        let remark = optional_str(&input, "notes");
        let require_min = input
            .get("require_min_notes")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize;
        let store = ScratchpadStore::open(context, &run_id)?;
        let scratchpad_cfg = context
            .runtime
            .wire
            .scratchpad_config
            .clone()
            .unwrap_or_default();
        let inventory =
            store.set_area_status(area_id, status, remark, require_min, &scratchpad_cfg)?;
        persist_scratchpad_run(context, &run_id);
        let areas_done = inventory
            .areas
            .iter()
            .filter(|a| a.status == AreaStatus::Done)
            .count();
        Ok(ToolResult::success(
            serde_json::to_string_pretty(&json!({
                "run_id": run_id,
                "area_id": area_id,
                "status": status.as_str(),
                "areas_done": areas_done,
            }))
            .unwrap_or_default(),
        ))
    }
}
