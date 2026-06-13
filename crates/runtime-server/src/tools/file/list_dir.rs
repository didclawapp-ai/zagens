//! list_dir tool.

use super::schemas::list_dir_input_schema;
use crate::tools::spec::{
    ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, optional_str, optional_u64,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::fs;

/// Default cap on entries returned so a huge directory can't flood the model.
const DEFAULT_LIST_LIMIT: usize = 1000;
const MAX_LIST_LIMIT: usize = 10000;

pub struct ListDirTool;

#[async_trait]
impl ToolSpec for ListDirTool {
    fn name(&self) -> &'static str {
        "list_dir"
    }

    fn description(&self) -> &'static str {
        "List entries in a directory relative to the workspace."
    }

    fn input_schema(&self) -> Value {
        list_dir_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let path_str = optional_str(&input, "path").unwrap_or(".");
        let dir_path = context.resolve_path(path_str)?;
        let limit = (optional_u64(&input, "limit", DEFAULT_LIST_LIMIT as u64) as usize)
            .clamp(1, MAX_LIST_LIMIT);
        let offset = optional_u64(&input, "offset", 0) as usize;

        // Collect first so we can sort deterministically (read_dir order is
        // filesystem-dependent and otherwise non-reproducible).
        let mut raw: Vec<(String, bool, bool)> = Vec::new();
        for entry in fs::read_dir(&dir_path).map_err(|e| {
            ToolError::execution_failed(format!(
                "Failed to read directory {}: {}",
                dir_path.display(),
                e
            ))
        })? {
            let entry = entry.map_err(|e| ToolError::execution_failed(e.to_string()))?;
            let file_type = entry
                .file_type()
                .map_err(|e| ToolError::execution_failed(e.to_string()))?;
            raw.push((
                entry.file_name().to_string_lossy().to_string(),
                file_type.is_dir(),
                file_type.is_symlink(),
            ));
        }

        // Directories first, then files; each group sorted by name.
        raw.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let total = raw.len();
        let offset = offset.min(total);
        let remaining = total.saturating_sub(offset);
        let truncated = remaining > limit;
        let page: Vec<_> = raw.into_iter().skip(offset).take(limit).collect();
        let returned = page.len();

        let entries: Vec<Value> = page
            .into_iter()
            .map(|(name, is_dir, is_symlink)| {
                json!({ "name": name, "is_dir": is_dir, "is_symlink": is_symlink })
            })
            .collect();

        let payload = json!({
            "path": dir_path.to_string_lossy(),
            "total": total,
            "offset": offset,
            "returned": returned,
            "truncated": truncated,
            "entries": entries,
        });

        ToolResult::json(&payload).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}
