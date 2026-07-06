//! T5 composite tool: `edit_and_check` — edit → LSP → optional run_tests (Phase 4.3).

use std::time::Instant;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::harness::affected_tests::{hint_suffix_for_tool, suggest_for_edited_paths};
use zagens_core::engine::edited_paths_for_tool;

use super::composite::{CompositeStep, composite_metadata};
use super::file::EditFileTool;
use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool, optional_str,
};
use super::test_runner::RunTestsTool;

pub struct EditAndCheckTool;

#[async_trait]
impl ToolSpec for EditAndCheckTool {
    fn name(&self) -> &'static str {
        "edit_and_check"
    }

    fn description(&self) -> &'static str {
        "Composite edit: edit_file → LSP diagnostics → optional run_tests (T5). \
         Applies an edit, surfaces diagnostics, and can run scoped tests in one call. \
         Forwards edit_file fields (path, search, replace, …) plus optional `run_tests` / `test_args`."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File to edit (edit_file.path)." },
                "search": { "type": "string" },
                "replace": { "type": "string" },
                "replace_mode": { "type": "string", "enum": ["first", "all"] },
                "start_line": { "type": "integer", "minimum": 1 },
                "end_line": { "type": "integer", "minimum": 1 },
                "run_tests": {
                    "type": "boolean",
                    "description": "When true (default), run scoped cargo test after a successful edit."
                },
                "test_args": {
                    "type": "string",
                    "description": "Extra args for run_tests (e.g. '-p my-crate --lib')."
                }
            },
            "required": ["path", "search", "replace"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::WritesFiles,
            ToolCapability::ExecutesCode,
            ToolCapability::Sandboxable,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let run_tests = optional_bool(&input, "run_tests", true);
        let test_args = optional_str(&input, "test_args").map(str::to_string);

        let mut steps: Vec<CompositeStep> = Vec::new();

        // Step 1: edit (includes LSP diagnostics in content)
        let edit_started = Instant::now();
        let edit_tool = EditFileTool;
        let edit_input = input.clone();
        let edit_result = edit_tool.execute(edit_input, context).await?;
        if !edit_result.success {
            steps.push(CompositeStep::fail(
                "edit_file",
                edit_started,
                edit_result.content.clone(),
            ));
            return Ok(edit_result.with_metadata(composite_metadata(&steps)));
        }
        steps.push(CompositeStep::ok(
            "edit_file",
            edit_started,
            summarize(&edit_result.content, 240),
        ));

        let mut body = edit_result.content.clone();

        if let Some(suffix) = hint_suffix_for_tool(context.workspace.as_path(), "edit_file", &input)
        {
            body.push_str(&suffix);
        }

        if !run_tests {
            return Ok(ToolResult::success(body).with_metadata(composite_metadata(&steps)));
        }

        // Step 2: run_tests with affected hint or caller args
        let tests_started = Instant::now();
        let tests_tool = RunTestsTool;
        let mut tests_input = json!({});
        if let Some(args) = test_args {
            tests_input["args"] = json!(args);
        } else {
            let paths = edited_paths_for_tool("edit_file", &input);
            if let Some(suggestion) = suggest_for_edited_paths(context.workspace.as_path(), &paths)
            {
                tests_input["args"] = json!(suggestion.run_tests_args);
            }
        }

        let tests_result = tests_tool.execute(tests_input, context).await?;
        if tests_result.success {
            steps.push(CompositeStep::ok(
                "run_tests",
                tests_started,
                summarize(&tests_result.content, 240),
            ));
            body.push_str("\n\n## run_tests\n");
            body.push_str(&tests_result.content);
            Ok(ToolResult::success(body).with_metadata(composite_metadata(&steps)))
        } else {
            steps.push(CompositeStep::fail(
                "run_tests",
                tests_started,
                tests_result.content.clone(),
            ));
            body.push_str("\n\n## run_tests (failed)\n");
            body.push_str(&tests_result.content);
            Ok(ToolResult::error(body).with_metadata(composite_metadata(&steps)))
        }
    }
}

fn summarize(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        format!("{}…", &text[..max])
    }
}
