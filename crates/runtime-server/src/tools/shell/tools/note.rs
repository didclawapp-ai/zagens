//! Agent notes ToolSpec.

use std::io::Write;

use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, required_str,
};
use async_trait::async_trait;
use serde_json::json;

/// Tool for appending notes to a notes file.
pub struct NoteTool;

#[async_trait]
impl ToolSpec for NoteTool {
    fn name(&self) -> &'static str {
        "note"
    }

    fn description(&self) -> &'static str {
        "Append a note to the agent notes file for persistent context across sessions."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The note content to append"
                }
            },
            "required": ["content"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::WritesFiles]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto // Notes are low-risk
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let note_content = required_str(&input, "content")?;

        // Ensure parent directory exists
        if let Some(parent) = context.notes_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ToolError::execution_failed(format!("Failed to create notes directory: {e}"))
            })?;
        }

        // Append to notes file
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&context.notes_path)
            .map_err(|e| ToolError::execution_failed(format!("Failed to open notes file: {e}")))?;

        writeln!(file, "\n---\n{note_content}")
            .map_err(|e| ToolError::execution_failed(format!("Failed to write note: {e}")))?;

        Ok(ToolResult::success(format!(
            "Note appended to {}",
            context.notes_path.display()
        )))
    }
}
