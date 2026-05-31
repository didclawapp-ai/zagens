//! Cargo test runner tool: `run_tests`.
//!
//! This tool intentionally auto-approves test execution to encourage
//! frequent verification loops while still scoping execution to the workspace.

use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool, optional_str, optional_u64,
};

const MAX_OUTPUT_CHARS: usize = 40_000;
/// Default wall-clock cap on a `cargo test` run (C6). Without it a hung test /
/// build blocks the tool — and the spawned process tree — indefinitely.
const DEFAULT_TIMEOUT_MS: u64 = 600_000;
/// Upper bound a caller may request via `timeout_ms`.
const HARD_MAX_TIMEOUT_MS: u64 = 1_800_000;

/// Tool for running `cargo test` in the workspace root.
pub struct RunTestsTool;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunTestsOutput {
    success: bool,
    exit_code: i32,
    stdout: String,
    stderr: String,
    command: String,
}

#[async_trait]
impl ToolSpec for RunTestsTool {
    fn name(&self) -> &'static str {
        "run_tests"
    }

    fn description(&self) -> &'static str {
        "Run `cargo test` in the workspace root with optional extra arguments."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "args": {
                    "type": "string",
                    "description": "Optional extra arguments to pass to `cargo test` (shell-style)."
                },
                "all_features": {
                    "type": "boolean",
                    "description": "When true, include `--all-features`."
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Wall-clock timeout in milliseconds before the test run is killed (default 600,000; max 1,800,000)."
                }
            },
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ExecutesCode, ToolCapability::Sandboxable]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        // Tests are encouraged, so avoid gating them behind approval.
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let all_features = optional_bool(&input, "all_features", false);
        let extra_args = optional_str(&input, "args")
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let mut args = vec!["test".to_string()];
        if all_features {
            args.push("--all-features".to_string());
        }
        if let Some(extra) = extra_args {
            let split = shlex::split(extra).ok_or_else(|| {
                ToolError::invalid_input("Failed to parse 'args' as shell-style tokens")
            })?;
            args.extend(split);
        }

        let timeout_ms =
            optional_u64(&input, "timeout_ms", DEFAULT_TIMEOUT_MS).min(HARD_MAX_TIMEOUT_MS);

        let command_str = format_command(&context.workspace, &args);
        let output = run_cargo(&context.workspace, &args, timeout_ms).await?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout_raw = String::from_utf8_lossy(&output.stdout);
        let stderr_raw = String::from_utf8_lossy(&output.stderr);
        let stdout = truncate_with_note(&stdout_raw, MAX_OUTPUT_CHARS);
        let stderr = truncate_with_note(&stderr_raw, MAX_OUTPUT_CHARS);

        let result = RunTestsOutput {
            success: output.status.success(),
            exit_code,
            stdout,
            stderr,
            command: command_str,
        };

        ToolResult::json(&result).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

// === Helpers ===

async fn run_cargo(
    workspace: &Path,
    args: &[String],
    timeout_ms: u64,
) -> Result<std::process::Output, ToolError> {
    use std::process::Stdio;
    // C4: async process so a long `cargo test` does not block a tokio worker.
    let mut cmd = tokio::process::Command::new("cargo");
    cmd.args(args)
        .current_dir(workspace)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // C6: if we drop the child (timeout below) make sure cargo itself is
        // killed rather than detaching into a background process.
        .kill_on_drop(true);
    let child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ToolError::not_available("cargo is not installed or not in PATH")
        } else {
            ToolError::execution_failed(format!("Failed to run cargo: {e}"))
        }
    })?;
    let pid = child.id();

    // C6: bound the run. On timeout the wait future is dropped (kill_on_drop
    // terminates cargo) and we additionally sweep the process tree so rustc /
    // test binaries cargo spawned don't linger as orphans.
    match tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        child.wait_with_output(),
    )
    .await
    {
        Ok(res) => {
            res.map_err(|e| ToolError::execution_failed(format!("Failed to run cargo: {e}")))
        }
        Err(_) => {
            if let Some(pid) = pid {
                kill_tree_best_effort(pid);
            }
            Err(ToolError::execution_failed(format!(
                "cargo test exceeded the {timeout_ms} ms timeout and was killed \
                 (raise `timeout_ms` if the suite legitimately needs longer)"
            )))
        }
    }
}

/// Best-effort kill of `pid` and its descendants. On Windows a dropped child
/// only ends cargo itself, leaving rustc / test binaries as orphans, so we
/// `taskkill /T`. On Unix `kill_on_drop` already SIGKILLs the spawned cargo.
#[cfg(windows)]
fn kill_tree_best_effort(pid: u32) {
    use std::process::{Command, Stdio};
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(windows))]
fn kill_tree_best_effort(_pid: u32) {}

fn format_command(workspace: &Path, args: &[String]) -> String {
    format!(
        "(cd {} && cargo {})",
        workspace.display(),
        args.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn truncate_with_note(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let end = char_boundary_index(text, max_chars);
    let truncated = &text[..end];
    let omitted_chars = text
        .chars()
        .count()
        .saturating_sub(truncated.chars().count());
    let note = format!(
        "\n\n[output truncated to {max_chars} characters; {omitted_chars} characters omitted]"
    );
    format!("{truncated}{note}")
}

fn char_boundary_index(text: &str, max_chars: usize) -> usize {
    if max_chars == 0 {
        return 0;
    }
    for (count, (idx, _)) in text.char_indices().enumerate() {
        if count == max_chars {
            return idx;
        }
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    fn cargo_available() -> bool {
        Command::new("cargo")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn init_cargo_project(root: &Path) -> std::path::PathBuf {
        let project_dir = root.join("project");
        fs::create_dir_all(&project_dir).expect("create project dir");
        let status = Command::new("cargo")
            .args([
                "init",
                "--lib",
                "--vcs",
                "none",
                "-q",
                "--name",
                "eval_project",
            ])
            .current_dir(&project_dir)
            .status()
            .expect("cargo should spawn");
        assert!(status.success(), "cargo init failed");
        project_dir
    }

    #[tokio::test]
    async fn run_tests_succeeds_on_fresh_project() {
        if !cargo_available() {
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let project_dir = init_cargo_project(tmp.path());

        let ctx = ToolContext::new(&project_dir);
        let tool = RunTestsTool;
        let result = tool.execute(json!({}), &ctx).await.expect("execute");
        assert!(result.success);

        let parsed: RunTestsOutput =
            serde_json::from_str(&result.content).expect("tool result should be json");
        assert!(parsed.success);
        assert_eq!(parsed.exit_code, 0);
        assert!(parsed.command.contains("cargo test"));
    }

    #[tokio::test]
    async fn run_tests_reports_failures_without_hard_error() {
        if !cargo_available() {
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let project_dir = init_cargo_project(tmp.path());

        let lib_rs = project_dir.join("src/lib.rs");
        let failing = r#"
pub fn add(a: i32, b: i32) -> i32 { a + b }

#[cfg(test)]
mod tests {
    #[test]
    fn fails() {
        assert_eq!(2 + 2, 5);
    }
}
"#;
        fs::write(&lib_rs, failing).expect("write failing test");

        let ctx = ToolContext::new(&project_dir);
        let tool = RunTestsTool;
        let result = tool.execute(json!({}), &ctx).await.expect("execute");
        assert!(result.success);

        let parsed: RunTestsOutput =
            serde_json::from_str(&result.content).expect("tool result should be json");
        assert!(!parsed.success);
        assert_ne!(parsed.exit_code, 0);
    }

    #[test]
    fn truncation_adds_note() {
        let long = "x".repeat(MAX_OUTPUT_CHARS + 128);
        let truncated = truncate_with_note(&long, MAX_OUTPUT_CHARS);
        assert!(truncated.contains("output truncated"));
    }

    /// Tool surface audit C6 — a too-small timeout must abort the run with a
    /// timeout error rather than blocking indefinitely.
    #[tokio::test]
    async fn run_tests_times_out_and_kills() {
        if !cargo_available() {
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let project_dir = init_cargo_project(tmp.path());

        let ctx = ToolContext::new(&project_dir);
        let tool = RunTestsTool;
        // 1ms is unreachable for a real `cargo test` (build alone takes longer).
        let result = tool
            .execute(json!({"timeout_ms": 1}), &ctx)
            .await;
        let err = result.expect_err("must time out");
        assert!(
            format!("{err}").contains("timeout"),
            "expected timeout error, got: {err}"
        );
    }
}
