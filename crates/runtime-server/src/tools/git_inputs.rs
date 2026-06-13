//! schemars input types for the git tool family (kernel-v2 M2).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::tools::tool_schema::derived_input_schema;

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
pub struct GitStatusInput {
    #[schemars(
        description = "Optional subdirectory or file to scope the status to (must be within the workspace)."
    )]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
pub struct GitDiffInput {
    #[schemars(
        description = "Optional subdirectory or file to scope the diff to (must be within the workspace)."
    )]
    pub path: Option<String>,
    #[schemars(description = "When true, diff staged changes (`--cached`).")]
    pub cached: Option<bool>,
    #[schemars(
        extend("minimum" = 0, "maximum" = 50, "default" = 3),
        description = "Number of context lines to include around changes."
    )]
    pub unified: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
pub struct GitLogInput {
    #[schemars(description = "Optional subdirectory or file path to scope history to.")]
    pub path: Option<String>,
    #[schemars(
        extend("minimum" = 1, "maximum" = 200, "default" = 20),
        description = "Maximum number of commits to return."
    )]
    pub max_count: Option<u64>,
    #[schemars(description = "Optional git author filter (same semantics as `git log --author`).")]
    pub author: Option<String>,
    #[schemars(description = "Optional lower date bound, e.g. '2 weeks ago' or ISO date.")]
    pub since: Option<String>,
    #[schemars(description = "Optional upper date bound, e.g. 'yesterday' or ISO date.")]
    pub until: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
pub struct GitShowInput {
    #[schemars(description = "Revision to show (commit SHA, tag, branch, or ref expression).")]
    pub rev: String,
    #[schemars(description = "Optional subdirectory or file path to scope output.")]
    pub path: Option<String>,
    #[schemars(
        extend("default" = true),
        description = "Include patch hunks (default true)."
    )]
    pub patch: Option<bool>,
    #[schemars(
        extend("default" = true),
        description = "Include --stat summary (default true)."
    )]
    pub stat: Option<bool>,
    #[schemars(
        extend("minimum" = 0, "maximum" = 50, "default" = 3),
        description = "Context lines for patch output when patch=true."
    )]
    pub unified: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
pub struct GitBlameInput {
    #[schemars(description = "Path to a tracked file within the workspace.")]
    pub path: String,
    #[schemars(description = "Optional revision to blame against (default: HEAD).")]
    pub rev: Option<String>,
    #[schemars(
        extend("minimum" = 1, "default" = 1),
        description = "First line to include in blame output."
    )]
    pub start_line: Option<u64>,
    #[schemars(
        extend("minimum" = 1, "maximum" = 2000, "default" = 200),
        description = "Maximum number of lines to include."
    )]
    pub max_lines: Option<u64>,
    #[schemars(
        extend("default" = false),
        description = "When true, emit `--line-porcelain` output."
    )]
    pub porcelain: Option<bool>,
}

#[must_use]
pub fn git_status_input_schema() -> Value {
    derived_input_schema::<GitStatusInput>()
}

#[must_use]
pub fn git_diff_input_schema() -> Value {
    derived_input_schema::<GitDiffInput>()
}

#[must_use]
pub fn git_log_input_schema() -> Value {
    derived_input_schema::<GitLogInput>()
}

#[must_use]
pub fn git_show_input_schema() -> Value {
    derived_input_schema::<GitShowInput>()
}

#[must_use]
pub fn git_blame_input_schema() -> Value {
    derived_input_schema::<GitBlameInput>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::git::{GitDiffTool, GitStatusTool};
    use crate::tools::git_history::{GitBlameTool, GitLogTool, GitShowTool};
    use crate::tools::schema_sanitize;
    use crate::tools::spec::ToolSpec;

    fn model_visible_input_schema(tool: &dyn ToolSpec) -> Value {
        let mut schema = tool.input_schema();
        schema_sanitize::sanitize(&mut schema);
        schema
    }

    const GIT_SCHEMA_SNAPSHOT_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/harness/kernel-v2-schema-snapshots"
    );

    #[test]
    #[ignore = "bootstrap kernel-v2 git-tool schema snapshot fixtures"]
    fn dump_git_tool_schemas_for_snapshot_bootstrap() {
        let tools: [(&str, Box<dyn ToolSpec>); 5] = [
            ("git_status", Box::new(GitStatusTool)),
            ("git_diff", Box::new(GitDiffTool)),
            ("git_log", Box::new(GitLogTool)),
            ("git_show", Box::new(GitShowTool)),
            ("git_blame", Box::new(GitBlameTool)),
        ];
        for (name, tool) in tools {
            let schema = model_visible_input_schema(tool.as_ref());
            let pretty = serde_json::to_string_pretty(&schema).expect("serialize");
            println!("=== {name} ===\n{pretty}\n");
        }
    }

    #[test]
    fn git_tool_model_visible_schemas_match_snapshots() {
        let tools: [(&str, Box<dyn ToolSpec>); 5] = [
            ("git_status", Box::new(GitStatusTool)),
            ("git_diff", Box::new(GitDiffTool)),
            ("git_log", Box::new(GitLogTool)),
            ("git_show", Box::new(GitShowTool)),
            ("git_blame", Box::new(GitBlameTool)),
        ];
        for (name, tool) in tools {
            assert_eq!(tool.name(), name);
            let schema = model_visible_input_schema(tool.as_ref());
            let path = format!("{GIT_SCHEMA_SNAPSHOT_DIR}/git-{name}.json");
            let expected: Value = serde_json::from_str(
                &std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("missing snapshot {path}: {e}")),
            )
            .expect("parse snapshot JSON");
            assert_eq!(
                schema, expected,
                "model-visible schema drift for {name} — update fixture only after explicit KV-cache review"
            );
        }
    }
}
