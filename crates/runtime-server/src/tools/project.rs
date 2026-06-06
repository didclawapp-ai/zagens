//! Project mapping tool for understanding codebase structure.

use crate::utils::{
    DEFAULT_PROJECT_TREE_LINE_LIMIT, is_key_file, project_tree_with_limit, summarize_project,
};
use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;
use serde_json::{Value, json};

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, optional_u64,
};

pub struct ProjectMapTool;

#[derive(Debug, Serialize)]
struct ProjectMap {
    tree: String,
    summary: String,
    key_files: Vec<String>,
    tree_total_lines: usize,
    tree_truncated: bool,
}

#[async_trait]
impl ToolSpec for ProjectMapTool {
    fn name(&self) -> &'static str {
        "project_map"
    }

    fn description(&self) -> &'static str {
        "Get a high-level map of the project structure, including key files and a tree view."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum depth for the tree view (default: 3)."
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let max_depth = optional_u64(&input, "max_depth", 3) as usize;
        let map = generate_project_map(&context.workspace, max_depth)?;
        ToolResult::json(&map).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

fn generate_project_map(root: &std::path::Path, max_depth: usize) -> Result<ProjectMap, ToolError> {
    let (tree, tree_meta) =
        project_tree_with_limit(root, max_depth, Some(DEFAULT_PROJECT_TREE_LINE_LIMIT));
    let summary = summarize_project(root);

    // For key_files, we can just do a quick scan since summarize_project doesn't return them directly anymore
    let mut key_files = Vec::new();
    let mut builder = ignore::WalkBuilder::new(root);
    builder.hidden(false).follow_links(false).max_depth(Some(2));
    let walker = builder.build();

    for entry in walker.flatten() {
        if is_key_file(entry.path())
            && let Ok(rel) = entry.path().strip_prefix(root)
        {
            key_files.push(rel.to_string_lossy().to_string());
        }
    }

    Ok(ProjectMap {
        tree,
        summary,
        key_files,
        tree_total_lines: tree_meta.total_lines,
        tree_truncated: tree_meta.truncated,
    })
}
