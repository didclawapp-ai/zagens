//! T4 harness assert tools (Phase 2a.4) — thin wrappers over `predicate::*` only.

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::long_horizon::predicate::{self, CompletionGateExec, PredicateContext};

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_str, optional_u64, required_str,
};

async fn run_predicate(
    predicate: &str,
    args: Value,
    context: &ToolContext,
) -> Result<predicate::PredicateResult, ToolError> {
    let exec = CompletionGateExec {
        shell_manager: &context.shell_manager,
        cancel_token: context.cancel_token.as_ref(),
    };
    let pred_ctx = PredicateContext {
        workspace: &context.workspace,
        timeout_ms: 300_000,
        exec: Some(&exec),
        run_id: format!("assert-{predicate}"),
    };
    predicate::evaluate(predicate, &args, &pred_ctx)
        .await
        .map_err(|e| ToolError::execution_failed(e.to_string()))
}

fn predicate_tool_result(
    tool_name: &str,
    predicate: &str,
    result: predicate::PredicateResult,
    stage: Option<&str>,
    extra: Value,
) -> ToolResult {
    let mut meta = json!({
        "predicate": predicate,
        "pass": result.pass,
        "duration_ms": result.duration_ms,
        "exit_code": result.exit_code,
    });
    if let Some(stage) = stage {
        meta["stage"] = json!(stage);
    }
    if let Some(ref code) = result.code {
        meta["code"] = json!(code);
    }
    if let Some(ref suggestion) = result.suggestion {
        meta["suggestion"] = json!(suggestion);
    }
    if let Some(obj) = extra.as_object() {
        for (k, v) in obj {
            meta[k] = v.clone();
        }
    }
    let summary = if result.pass {
        format!("{tool_name}: pass ({predicate})")
    } else {
        format!(
            "{tool_name}: fail ({predicate}) — {}",
            result
                .suggestion
                .as_deref()
                .or(result.code.as_deref())
                .unwrap_or("predicate failed")
        )
    };
    ToolResult::success(summary).with_metadata(meta)
}

pub struct AssertFileCountTool;

#[async_trait]
impl ToolSpec for AssertFileCountTool {
    fn name(&self) -> &'static str {
        "assert_file_count"
    }

    fn description(&self) -> &'static str {
        "Assert workspace glob match count is within min/max bounds (predicate `file_count` only)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "glob": { "type": "string", "description": "Glob pattern relative to workspace." },
                "min": { "type": "integer", "minimum": 0 },
                "max": { "type": "integer", "minimum": 0 },
                "stage": { "type": "string", "description": "Optional skill stage id to mark verified on pass." }
            },
            "required": ["glob"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let glob = required_str(&input, "glob")?;
        let stage = optional_str(&input, "stage");
        let mut args = json!({ "glob": glob });
        let min = optional_u64(&input, "min", 0);
        if min > 0 {
            args["min"] = json!(min);
        }
        if let Some(max) = input.get("max").and_then(Value::as_u64) {
            args["max"] = json!(max);
        }
        let result = run_predicate(predicate::names::FILE_COUNT, args, context).await?;
        Ok(predicate_tool_result(
            self.name(),
            predicate::names::FILE_COUNT,
            result,
            stage,
            json!({}),
        ))
    }
}

pub struct AssertOutputMatchesTool;

#[async_trait]
impl ToolSpec for AssertOutputMatchesTool {
    fn name(&self) -> &'static str {
        "assert_output_matches"
    }

    fn description(&self) -> &'static str {
        "Assert command output matches an expected pattern (predicate `command_output_matches` only)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" },
                "pattern": { "type": "string" },
                "stage": { "type": "string" }
            },
            "required": ["command"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let command = required_str(&input, "command")?;
        let stage = optional_str(&input, "stage");
        let mut args = json!({ "command": command });
        if let Some(pattern) = optional_str(&input, "pattern") {
            args["pattern"] = json!(pattern);
        }
        let result = run_predicate(predicate::names::COMMAND_OUTPUT_MATCHES, args, context).await?;
        Ok(predicate_tool_result(
            self.name(),
            predicate::names::COMMAND_OUTPUT_MATCHES,
            result,
            stage,
            json!({ "command": command }),
        ))
    }
}

pub struct AssertTestsPassTool;

#[async_trait]
impl ToolSpec for AssertTestsPassTool {
    fn name(&self) -> &'static str {
        "assert_tests_pass"
    }

    fn description(&self) -> &'static str {
        "Assert tests/build command exits 0 (predicate `tests_pass` only)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "cmd": { "type": "string" },
                "command": { "type": "string" },
                "toolchain": { "type": "string", "enum": ["auto", "cargo", "go", "rust"] },
                "package": { "type": "string" },
                "stage": { "type": "string" }
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
        let stage = optional_str(&input, "stage");
        let result = run_predicate(predicate::names::TESTS_PASS, input.clone(), context).await?;
        Ok(predicate_tool_result(
            self.name(),
            predicate::names::TESTS_PASS,
            result,
            stage,
            json!({}),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn assert_file_count_delegates_to_predicate() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"x").unwrap();
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let tool = AssertFileCountTool;
        let out = tool
            .execute(json!({"glob": "*.txt", "min": 1}), &ctx)
            .await
            .unwrap();
        assert!(out.content.contains("pass"));
        let meta = out.metadata.expect("metadata");
        assert_eq!(meta["pass"], true);
        assert_eq!(meta["predicate"], predicate::names::FILE_COUNT);
    }
}
