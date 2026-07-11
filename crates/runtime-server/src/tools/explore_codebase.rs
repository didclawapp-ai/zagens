//! T5 composite tool: `explore_codebase` — glob → grep → read (Phase 4.3).

use std::time::Instant;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::composite::{CompositeStep, composite_metadata};
use super::file::ReadFileTool;
use super::glob_files::GlobFilesTool;
use super::search::GrepFilesTool;
use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_str, optional_u64, required_str,
};

const DEFAULT_READ_LIMIT: u64 = 3;
const MAX_READ_LIMIT: u64 = 5;

pub struct ExploreCodebaseTool;

#[async_trait]
impl ToolSpec for ExploreCodebaseTool {
    fn name(&self) -> &'static str {
        "explore_codebase"
    }

    fn description(&self) -> &'static str {
        "Composite explore: glob_files → grep_files → read_file (T5). \
         Finds paths by glob, searches content, then reads top matching files in one call. \
         Prefer over chaining three separate tools when discovering code."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "glob_pattern": {
                    "type": "string",
                    "description": "Glob for filename discovery (e.g. '**/*.rs')."
                },
                "grep_pattern": {
                    "type": "string",
                    "description": "Regex searched inside glob-matched files."
                },
                "path": {
                    "type": "string",
                    "description": "Workspace-relative base for glob (default '.')."
                },
                "read_limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 5,
                    "description": "Max files to read after grep (default 3)."
                },
                "grep_output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "grep_files output_mode (default files_with_matches)."
                }
            },
            "required": ["glob_pattern", "grep_pattern"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let glob_pattern = required_str(&input, "glob_pattern")?;
        let grep_pattern = required_str(&input, "grep_pattern")?;
        let base_path = optional_str(&input, "path").unwrap_or(".");
        let read_limit = optional_u64(&input, "read_limit", DEFAULT_READ_LIMIT).min(MAX_READ_LIMIT);
        let grep_mode = optional_str(&input, "grep_output_mode").unwrap_or("files_with_matches");

        let mut steps: Vec<CompositeStep> = Vec::new();

        // Step 1: glob
        let glob_started = Instant::now();
        let glob_tool = GlobFilesTool;
        let glob_input = json!({
            "pattern": glob_pattern,
            "path": base_path,
            "limit": 100
        });
        let glob_result = glob_tool.execute(glob_input, context).await?;
        if !glob_result.success {
            let err = glob_result.content.clone();
            steps.push(CompositeStep::fail("glob_files", glob_started, err.clone()));
            return Ok(
                ToolResult::error(format!("explore_codebase: glob_files failed — {err}"))
                    .with_metadata(composite_metadata(&steps)),
            );
        }
        steps.push(CompositeStep::ok(
            "glob_files",
            glob_started,
            summarize(&glob_result.content, 240),
        ));

        let glob_json: Value = serde_json::from_str(&glob_result.content)
            .map_err(|e| ToolError::execution_failed(format!("glob JSON parse: {e}")))?;
        let glob_paths: Vec<String> = glob_json
            .get("files")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| e.get("path").and_then(|p| p.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        if glob_paths.is_empty() {
            let body = format!(
                "explore_codebase: glob `{glob_pattern}` matched 0 files under `{base_path}`."
            );
            return Ok(ToolResult::success(body).with_metadata(composite_metadata(&steps)));
        }

        // Step 2: grep (scoped to glob hits via include filter)
        let grep_started = Instant::now();
        let grep_tool = GrepFilesTool;
        let grep_input = json!({
            "pattern": grep_pattern,
            "path": base_path,
            "output_mode": grep_mode,
            "include": glob_paths,
            "max_results": 50
        });
        let grep_result = grep_tool.execute(grep_input, context).await?;
        if !grep_result.success {
            let err = grep_result.content.clone();
            steps.push(CompositeStep::fail("grep_files", grep_started, err.clone()));
            return Ok(
                ToolResult::error(format!("explore_codebase: grep_files failed — {err}"))
                    .with_metadata(composite_metadata(&steps)),
            );
        }
        steps.push(CompositeStep::ok(
            "grep_files",
            grep_started,
            summarize(&grep_result.content, 240),
        ));

        let read_targets = extract_read_targets(&grep_result.content, &glob_paths, read_limit);

        if read_targets.is_empty() {
            let body = format!(
                "explore_codebase: glob matched {} paths; grep `{grep_pattern}` found no reads.\n\n## grep\n{}",
                glob_paths.len(),
                grep_result.content
            );
            return Ok(ToolResult::success(body).with_metadata(composite_metadata(&steps)));
        }

        // Step 3: read top files
        let read_tool = ReadFileTool;
        let mut sections = Vec::new();
        for path in &read_targets {
            let read_started = Instant::now();
            let read_input = json!({ "path": path, "limit": 120 });
            match read_tool.execute(read_input, context).await {
                Ok(read_result) => {
                    if read_result.success {
                        steps.push(CompositeStep::ok(
                            "read_file",
                            read_started,
                            format!("{path} ({} chars)", read_result.content.len()),
                        ));
                        sections.push(format!("## read_file: {path}\n{}", read_result.content));
                    } else {
                        let err = read_result.content.clone();
                        steps.push(CompositeStep::fail("read_file", read_started, err.clone()));
                        sections.push(format!("## read_file: {path}\n(error) {err}"));
                    }
                }
                Err(e) => {
                    steps.push(CompositeStep::fail(
                        "read_file",
                        read_started,
                        e.to_string(),
                    ));
                    sections.push(format!("## read_file: {path}\n(error) {e}"));
                }
            }
        }

        let body = format!(
            "explore_codebase complete (glob `{glob_pattern}` → grep `{grep_pattern}` → {} reads).\n\n## grep summary\n{}\n\n{}",
            read_targets.len(),
            summarize(&grep_result.content, 800),
            sections.join("\n\n")
        );
        Ok(ToolResult::success(body).with_metadata(composite_metadata(&steps)))
    }
}

fn summarize(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        format!("{}…", &text[..max])
    }
}

fn extract_read_targets(grep_content: &str, glob_paths: &[String], limit: u64) -> Vec<String> {
    if let Ok(value) = serde_json::from_str::<Value>(grep_content) {
        if let Some(paths) = value.get("files").and_then(|v| v.as_array()) {
            return paths
                .iter()
                .filter_map(|p| p.as_str().map(str::to_string))
                .take(limit as usize)
                .collect();
        }
        if let Some(matches) = value.get("matches").and_then(|v| v.as_array()) {
            let mut out = Vec::new();
            for m in matches {
                if let Some(path) = m.get("path").and_then(|p| p.as_str())
                    && !out.iter().any(|existing: &String| existing == path)
                {
                    out.push(path.to_string());
                    if out.len() >= limit as usize {
                        break;
                    }
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
    }

    glob_paths.iter().take(limit as usize).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_read_targets_prefers_grep_files_array() {
        let grep = r#"{"files":["src/a.rs","src/b.rs"]}"#;
        let targets = extract_read_targets(grep, &["other.rs".into()], 2);
        assert_eq!(targets, vec!["src/a.rs", "src/b.rs"]);
    }
}
