//! Glob file finder: `glob_files` — match paths by glob pattern (ripgrep-style walk).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use async_trait::async_trait;
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Serialize;
use serde_json::{Value, json};

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool, optional_str, optional_u64, required_str,
};
use super::workspace_walk::collect_workspace_files;

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 100;

#[derive(Debug, Clone, Serialize)]
struct GlobFileEntry {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified_secs: Option<u64>,
}

pub struct GlobFilesTool;

fn build_glob_set(pattern: &str) -> Result<GlobSet, ToolError> {
    let normalized = pattern.replace('\\', "/");
    let mut builder = GlobSetBuilder::new();
    builder.add(
        Glob::new(&normalized)
            .map_err(|e| ToolError::invalid_input(format!("Invalid glob pattern: {e}")))?,
    );
    builder
        .build()
        .map_err(|e| ToolError::invalid_input(format!("Invalid glob pattern: {e}")))
}

fn path_matches_glob(glob_set: &GlobSet, relative: &str) -> bool {
    let posix = relative.replace('\\', "/");
    glob_set.is_match(&posix)
}

fn modified_secs(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

#[async_trait]
impl ToolSpec for GlobFilesTool {
    fn name(&self) -> &'static str {
        "glob_files"
    }

    fn description(&self) -> &'static str {
        "Find files by glob pattern (e.g. '**/*login*.{ts,tsx}'). Results sorted by modification time (newest first), max 100 paths. Use for filename discovery; use file_search for fuzzy path queries; use grep_files for content search."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern relative to path (e.g. '**/*.rs', 'src/**/*login*.tsx'). Use forward slashes."
                },
                "path": {
                    "type": "string",
                    "description": "Base directory (relative to workspace, default: .)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum files to return (default: 100, max: 100)"
                },
                "respect_gitignore": {
                    "type": "boolean",
                    "description": "Honor .gitignore when true (default: true)"
                }
            },
            "required": ["pattern"]
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
        let pattern = required_str(&input, "pattern")?;
        let path_str = optional_str(&input, "path").unwrap_or(".");
        let limit = usize::try_from(optional_u64(&input, "limit", DEFAULT_LIMIT as u64))
            .unwrap_or(DEFAULT_LIMIT)
            .min(MAX_LIMIT);
        let respect_gitignore = optional_bool(&input, "respect_gitignore", true);

        let base_path = context.resolve_path(path_str)?;
        if !base_path.exists() {
            return Err(ToolError::invalid_input(format!(
                "Path does not exist: {path_str}"
            )));
        }
        let glob_set = build_glob_set(pattern)?;

        let files = collect_workspace_files(&base_path, respect_gitignore);
        let mut matches: Vec<(PathBuf, String, Option<u64>)> = Vec::new();

        for path in files {
            // Match pattern relative to `path` (base_path), not workspace root —
            // so `path:"src"` + `*.rs` hits `src/foo.rs` without needing `**/*.rs`.
            let glob_relative = path
                .strip_prefix(&base_path)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if !path_matches_glob(&glob_set, &glob_relative) {
                continue;
            }
            let workspace_rel = path
                .strip_prefix(&context.workspace)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let mtime = modified_secs(&path);
            matches.push((path, workspace_rel, mtime));
        }

        matches.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
        let total = matches.len();
        let truncated = total > limit;
        if truncated {
            matches.truncate(limit);
        }

        let files_out: Vec<GlobFileEntry> = matches
            .into_iter()
            .map(|(_, rel, mtime)| GlobFileEntry {
                path: rel,
                modified_secs: mtime,
            })
            .collect();

        ToolResult::json(&json!({
            "pattern": pattern,
            "files": files_out,
            "count": files_out.len(),
            "total_matches": total,
            "truncated": truncated,
            "respect_gitignore": respect_gitignore,
        }))
        .map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    use crate::tools::spec::ToolSpec;

    #[tokio::test]
    async fn glob_files_finds_by_pattern_and_sorts_mtime() {
        let tmp = tempdir().expect("tempdir");
        fs::write(tmp.path().join("old_login.ts"), "x").expect("write");
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(tmp.path().join("new_login.tsx"), "y").expect("write");
        fs::write(tmp.path().join("readme.md"), "z").expect("write");

        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = GlobFilesTool;
        let result = tool
            .execute(json!({"pattern": "*login*"}), &ctx)
            .await
            .expect("execute");

        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        let files = parsed["files"].as_array().unwrap();
        assert_eq!(files.len(), 2);
        let first = files[0]["path"].as_str().unwrap();
        assert!(first.contains("new_login.tsx"));
    }

    #[tokio::test]
    async fn glob_files_respects_gitignore() {
        let tmp = tempdir().expect("tempdir");
        fs::write(tmp.path().join(".gitignore"), "ignored/\n").expect("write");
        fs::create_dir_all(tmp.path().join("ignored")).expect("mkdir");
        fs::write(tmp.path().join("ignored/secret.ts"), "x").expect("write");
        fs::write(tmp.path().join("visible.ts"), "y").expect("write");

        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = GlobFilesTool;
        let result = tool
            .execute(json!({"pattern": "**/*.ts"}), &ctx)
            .await
            .expect("execute");

        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        let paths: Vec<&str> = parsed["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v["path"].as_str())
            .collect();
        assert!(paths.iter().any(|p| p.contains("visible.ts")));
        assert!(!paths.iter().any(|p| p.contains("ignored")));
    }

    #[tokio::test]
    async fn glob_files_pattern_relative_to_path_not_workspace() {
        let tmp = tempdir().expect("tempdir");
        fs::create_dir_all(tmp.path().join("src")).expect("mkdir");
        fs::write(tmp.path().join("src").join("main.rs"), "x").expect("write");
        fs::write(tmp.path().join("root.rs"), "y").expect("write");

        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = GlobFilesTool;
        let result = tool
            .execute(json!({"pattern": "*.rs", "path": "src"}), &ctx)
            .await
            .expect("execute");

        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        let paths: Vec<&str> = parsed["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v["path"].as_str())
            .collect();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].contains("main.rs"));
        assert!(!paths.iter().any(|p| p.contains("root.rs")));
    }

    #[tokio::test]
    async fn glob_files_errors_when_path_missing() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = GlobFilesTool;
        let err = tool
            .execute(json!({"pattern": "*.rs", "path": "no_such_dir"}), &ctx)
            .await
            .expect_err("missing path should error");
        assert!(!err.to_string().is_empty());
    }
}
