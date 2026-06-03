//! `load_office_payload` — retrieve cached write_office JSON for iterative edits.

use async_trait::async_trait;
use serde_json::{Value, json};
use super::office_common::{load_office_payload_file, workspace_rel_path};
use super::spec::{
    ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, required_str,
};

pub struct LoadOfficePayloadTool;

#[async_trait]
impl ToolSpec for LoadOfficePayloadTool {
    fn name(&self) -> &'static str {
        "load_office_payload"
    }

    fn description(&self) -> &'static str {
        "Load the cached write_office JSON payload for a previously generated file under deliverables/. \
         Use before incremental edits: load payload → modify sheets/blocks/slides → write_office with the same path."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the office file (e.g. deliverables/report.pptx)"
                }
            },
            "required": ["path"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let path_str = required_str(&input, "path")?;
        let file_path = context.resolve_path(path_str)?;
        let payload = load_office_payload_file(&context.workspace, &file_path)?;
        let rel = workspace_rel_path(&context.workspace, &file_path);
        Ok(
            ToolResult::success(serde_json::to_string_pretty(&payload).map_err(|e| {
                ToolError::execution_failed(format!("序列化 payload 失败: {e}"))
            })?)
            .with_metadata(json!({
                "path": rel,
                "cache": format!("deliverables/.office/{}.payload.json",
                    file_path.file_name().and_then(|n| n.to_str()).unwrap_or("file")),
            })),
        )
    }
}
