//! Audit scratchpad tools (`scratchpad_status`, `scratchpad_append`, …).

use crate::tools::scratchpad_inputs::{
    scratchpad_append_input_schema, scratchpad_defer_remaining_input_schema,
    scratchpad_init_input_schema, scratchpad_list_notes_input_schema,
    scratchpad_set_area_input_schema, scratchpad_status_input_schema,
    scratchpad_verify_note_input_schema,
};
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::scratchpad::AreaStatus;
use crate::scratchpad::{
    ScratchpadStore, default_init_areas, display_run_path, parse_init_areas, resolve_run_id,
    resolve_run_id_for_init, verify_note, workspace_audit_inventory,
};

fn persist_scratchpad_run(ctx: &ToolContext, run_id: &str) {
    if let Ok(mut guard) = ctx.runtime.wire.scratchpad_run_id.lock() {
        *guard = Some(run_id.to_string());
    }
    if let Some(persist) = &ctx.runtime.wire.persist_scratchpad_run_id {
        persist(run_id.to_string());
    }
}
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_str, required_str,
};
#[derive(Debug, Default)]
pub struct ScratchpadInitTool;

#[async_trait]
impl ToolSpec for ScratchpadInitTool {
    fn name(&self) -> &'static str {
        "scratchpad_init"
    }

    fn description(&self) -> &'static str {
        "Bootstrap an audit scratchpad run under .zagens/scratchpad/{run_id}/ (inventory.json + notes.jsonl). Idempotent when inventory already exists."
    }

    fn input_schema(&self) -> Value {
        scratchpad_init_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::WritesFiles]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let run_id = resolve_run_id_for_init(context, optional_str(&input, "run_id"))?;
        let scope = optional_str(&input, "scope");
        let areas = if optional_str(&input, "template") == Some("workspace_audit") {
            workspace_audit_inventory(&context.workspace)?
        } else {
            match input.get("areas").and_then(|v| v.as_array()) {
                Some(raw) => parse_init_areas(raw)?,
                None => default_init_areas(),
            }
        };
        let store = ScratchpadStore::init(context, &run_id, areas, scope)?;
        persist_scratchpad_run(context, &run_id);
        let status = store.build_status()?;
        Ok(ToolResult::success(
            serde_json::to_string_pretty(&json!({
                "run_id": run_id,
                "path": display_run_path(&run_id),
                "status": status,
            }))
            .unwrap_or_default(),
        ))
    }
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
        scratchpad_status_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
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
        scratchpad_append_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::WritesFiles]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
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
        scratchpad_list_notes_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
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
        "Update one inventory area status. status=done defaults require_min_notes=1; status=deferred defaults require_min_notes=0 (still needs kind=meta when require_deferred_meta is enabled). Non-empty `notes` satisfies require_min_notes via an implicit meta append when notes.jsonl is empty."
    }

    fn input_schema(&self) -> Value {
        scratchpad_set_area_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::WritesFiles]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
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
            .map(|v| v as usize)
            .unwrap_or_else(|| match status {
                AreaStatus::Done => 1,
                _ => 0,
            });
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

#[derive(Debug, Default)]
pub struct ScratchpadDeferRemainingTool;

#[async_trait]
impl ToolSpec for ScratchpadDeferRemainingTool {
    fn name(&self) -> &'static str {
        "scratchpad_defer_remaining"
    }

    fn description(&self) -> &'static str {
        "P2 mass closeout: append kind=meta defer reason and set status=deferred for all pending \
         inventory areas (or an explicit area_ids list) in one call. Prefer this over batching \
         multiple scratchpad_set_area(deferred) which the runtime rejects."
    }

    fn input_schema(&self) -> Value {
        scratchpad_defer_remaining_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::WritesFiles]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let run_id = resolve_run_id(context, optional_str(&input, "run_id"))?;
        let reason_prefix = required_str(&input, "reason_prefix")?;
        let area_ids: Option<Vec<String>> =
            input.get("area_ids").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            });
        let store = ScratchpadStore::open(context, &run_id)?;
        let scratchpad_cfg = context
            .runtime
            .wire
            .scratchpad_config
            .clone()
            .unwrap_or_default();
        let outcome = store.defer_remaining(reason_prefix, area_ids.as_deref(), &scratchpad_cfg)?;
        persist_scratchpad_run(context, &run_id);
        let status = store.build_status()?;
        Ok(ToolResult::success(
            serde_json::to_string_pretty(&json!({
                "run_id": run_id,
                "deferred_count": outcome.deferred_area_ids.len(),
                "deferred_area_ids": outcome.deferred_area_ids,
                "skipped_already_closed": outcome.skipped_already_closed,
                "areas_pending": outcome.areas_pending,
                "status": status,
            }))
            .unwrap_or_default(),
        ))
    }
}

#[derive(Debug, Default)]
pub struct ScratchpadVerifyNoteTool;

#[async_trait]
impl ToolSpec for ScratchpadVerifyNoteTool {
    fn name(&self) -> &'static str {
        "scratchpad_verify_note"
    }

    fn description(&self) -> &'static str {
        "Promote an open scratchpad note to status=verified (append-only supersede). \
         Call only after read_file/grep_files confirms the claim. Required before scratchpad_set_area(done) when open HIGH/BLOCKER findings exist."
    }

    fn input_schema(&self) -> Value {
        scratchpad_verify_note_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::WritesFiles]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let run_id = resolve_run_id(context, optional_str(&input, "run_id"))?;
        let note_id = required_str(&input, "note_id")?;
        let store = ScratchpadStore::open(context, &run_id)?;
        let note = verify_note(&store, note_id)?;
        persist_scratchpad_run(context, &run_id);
        Ok(ToolResult::success(
            serde_json::to_string_pretty(&json!({
                "verified_id": note.id,
                "supersedes": note_id,
                "status": note.status,
            }))
            .unwrap_or_default(),
        ))
    }
}
