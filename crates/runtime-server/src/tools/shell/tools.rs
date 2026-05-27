//! Shell-related ToolSpec implementations.

use std::collections::HashMap;
use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

use super::types::{ShellDeltaResult, ShellResult, ShellStatus};
use crate::command_safety::{SafetyLevel, analyze_command};
use crate::execpolicy::{ExecPolicyDecision, load_default_policy};
use crate::features::Feature;
use crate::sandbox::SandboxPolicy as ExecutionSandboxPolicy;
use crate::tools::shell_output::{summarize_output, truncate_with_meta};
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool, optional_u64, required_str,
};
use async_trait::async_trait;
use serde_json::json;

const FOREGROUND_TIMEOUT_RECOVERY_HINT: &str = "Foreground exec_shell is for bounded commands. \
The timed-out process was killed; rerun long work with task_shell_start or exec_shell with \
background: true, then poll with task_shell_wait or exec_shell_wait.";

fn emit_shell_snapshot_stream(
    context: &ToolContext,
    snapshot: &ShellResult,
    prev_stdout_len: &mut usize,
    prev_stderr_len: &mut usize,
) {
    let Some(sink) = context.tool_progress.as_ref() else {
        return;
    };
    let stdout = &snapshot.stdout;
    let stderr = &snapshot.stderr;
    if stdout.len() > *prev_stdout_len {
        sink.emit_stdout(&stdout[*prev_stdout_len..]);
        *prev_stdout_len = stdout.len();
    }
    if stderr.len() > *prev_stderr_len {
        sink.emit_stderr(&stderr[*prev_stderr_len..]);
        *prev_stderr_len = stderr.len();
    }
}

fn emit_shell_delta_streams(context: &ToolContext, result: &ShellResult) {
    let Some(sink) = context.tool_progress.as_ref() else {
        return;
    };
    if !result.stdout.is_empty() {
        sink.emit_stdout(&result.stdout);
    }
    if !result.stderr.is_empty() {
        sink.emit_stderr(&result.stderr);
    }
}

async fn execute_foreground_via_background(
    context: &ToolContext,
    command: &str,
    timeout_ms: u64,
    stdin_data: Option<&str>,
    policy_override: Option<ExecutionSandboxPolicy>,
    extra_env: HashMap<String, String>,
) -> Result<ShellResult> {
    let timeout_ms = timeout_ms.clamp(1000, 600_000);
    let spawned = {
        let mut manager = context
            .shell_manager
            .lock()
            .map_err(|_| anyhow!("shell manager lock poisoned"))?;
        manager.clear_foreground_background_request();
        manager.execute_with_options_env(
            command,
            None,
            timeout_ms,
            true,
            stdin_data,
            false,
            policy_override,
            extra_env,
        )?
    };
    let task_id = spawned
        .task_id
        .ok_or_else(|| anyhow!("foreground shell did not return a process id"))?;

    if stdin_data.is_some() {
        let mut manager = context
            .shell_manager
            .lock()
            .map_err(|_| anyhow!("shell manager lock poisoned"))?;
        manager.write_stdin(&task_id, "", true)?;
    }

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut prev_stdout_len = 0usize;
    let mut prev_stderr_len = 0usize;
    loop {
        if context
            .cancel_token
            .as_ref()
            .is_some_and(|token| token.is_cancelled())
        {
            let mut manager = context
                .shell_manager
                .lock()
                .map_err(|_| anyhow!("shell manager lock poisoned"))?;
            return manager.kill(&task_id);
        }

        let snapshot = {
            let mut manager = context
                .shell_manager
                .lock()
                .map_err(|_| anyhow!("shell manager lock poisoned"))?;
            if manager.take_foreground_background_request() {
                return manager.get_output(&task_id, false, 0);
            }
            manager.get_output(&task_id, false, 0)?
        };

        emit_shell_snapshot_stream(
            context,
            &snapshot,
            &mut prev_stdout_len,
            &mut prev_stderr_len,
        );

        if snapshot.status != ShellStatus::Running {
            return Ok(snapshot);
        }

        if Instant::now() >= deadline {
            let mut manager = context
                .shell_manager
                .lock()
                .map_err(|_| anyhow!("shell manager lock poisoned"))?;
            let mut result = manager.kill(&task_id)?;
            result.status = ShellStatus::TimedOut;
            return Ok(result);
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Tool for executing shell commands.
pub struct ExecShellTool;

#[async_trait]
impl ToolSpec for ExecShellTool {
    fn name(&self) -> &'static str {
        "exec_shell"
    }

    fn description(&self) -> &'static str {
        "Execute a shell command in the workspace directory. Foreground mode is for bounded commands; use background=true or task_shell_start for long-running work, then poll/wait."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default: 120000, max: 600000)"
                },
                "background": {
                    "type": "boolean",
                    "description": "Run in background and return task_id (default: false). Prefer true for commands that may run for minutes; poll with exec_shell_wait or task_shell_wait."
                },
                "interactive": {
                    "type": "boolean",
                    "description": "Run interactively with terminal IO (default: false)"
                },
                "stdin": {
                    "type": "string",
                    "description": "Optional stdin data to send before waiting (non-interactive only)"
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional working directory for the command"
                },
                "tty": {
                    "type": "boolean",
                    "description": "Allocate a pseudo-terminal for interactive programs (implies background)"
                }
            },
            "required": ["command"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::ExecutesCode,
            ToolCapability::Sandboxable,
            ToolCapability::RequiresApproval,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let command = required_str(&input, "command")?;
        let timeout_ms = optional_u64(&input, "timeout_ms", 120_000).min(600_000);
        let background = optional_bool(&input, "background", false);
        let interactive = optional_bool(&input, "interactive", false);
        let tty = optional_bool(&input, "tty", false);
        let stdin_data = input
            .get("stdin")
            .or_else(|| input.get("input"))
            .or_else(|| input.get("data"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);

        if interactive && background {
            return Ok(ToolResult::error(
                "Interactive commands cannot run in background mode.",
            ));
        }
        if interactive && tty {
            return Ok(ToolResult::error(
                "Interactive mode cannot be combined with TTY sessions.",
            ));
        }
        if interactive && stdin_data.is_some() {
            return Ok(ToolResult::error(
                "Interactive mode cannot be combined with stdin data.",
            ));
        }

        let background = background || tty;

        let mut execpolicy_decision: Option<ExecPolicyDecision> = None;
        if context.features.enabled(Feature::ExecPolicy)
            && let Some(policy) = load_default_policy()
                .map_err(|e| ToolError::execution_failed(format!("execpolicy load failed: {e}")))?
        {
            let decision = policy.evaluate(command);
            execpolicy_decision = Some(decision.clone());
            if let ExecPolicyDecision::Deny(reason) = decision {
                return Ok(ToolResult {
                    content: format!("BLOCKED: {reason}"),
                    success: false,
                    metadata: Some(json!({
                        "execpolicy": {
                            "decision": "deny",
                            "reason": reason,
                        }
                    })),
                });
            }
        }

        // Safety analysis (always run for metadata, but only block when not in YOLO mode)
        let safety = analyze_command(command);
        if !context.auto_approve {
            match safety.level {
                SafetyLevel::Dangerous => {
                    let reasons = safety.reasons.join("; ");
                    let suggestions = if safety.suggestions.is_empty() {
                        String::new()
                    } else {
                        format!("\nSuggestions: {}", safety.suggestions.join("; "))
                    };
                    return Ok(ToolResult {
                        content: format!(
                            "BLOCKED: This command was blocked for safety reasons.\n\nReasons: {reasons}{suggestions}"
                        ),
                        success: false,
                        metadata: Some(json!({
                            "safety_level": "dangerous",
                            "blocked": true,
                            "reasons": safety.reasons,
                            "suggestions": safety.suggestions,
                        })),
                    });
                }
                SafetyLevel::RequiresApproval | SafetyLevel::Safe | SafetyLevel::WorkspaceSafe => {
                    // Proceed normally
                }
            }
        }

        let policy_override = context.elevated_sandbox_policy.clone();
        let working_dir = match input
            .get("cwd")
            .or_else(|| input.get("working_dir"))
            .and_then(serde_json::Value::as_str)
        {
            Some(dir) => {
                // Validate cwd against workspace boundary (same as file tools)
                let resolved = context.resolve_path(dir)?;
                Some(resolved.to_string_lossy().to_string())
            }
            None => None,
        };

        // #456 — collect env from any configured `shell_env` hooks. Runs
        // synchronously, captures stdout, parses `KEY=VAL` lines, audit-logs
        // the keys (never the values). Empty / no-op when no hook is
        // configured.
        let extra_env = if let Some(shell_env) = &context.runtime.shell_env {
            shell_env.collect_shell_env("exec_shell", &input)
        } else {
            std::collections::HashMap::new()
        };

        // Route through external sandbox backend when configured.
        if let Some(backend) = &context.sandbox_backend {
            if interactive {
                return Ok(ToolResult::error(
                    "Interactive mode is not supported with external sandbox backends.",
                ));
            }
            if background {
                return Ok(ToolResult::error(
                    "Background mode is not supported with external sandbox backends.",
                ));
            }
            if tty {
                return Ok(ToolResult::error(
                    "TTY mode is not supported with external sandbox backends.",
                ));
            }

            let started = std::time::Instant::now();
            let backend_result = backend.exec(command, &extra_env).await;

            let result = match backend_result {
                Ok(output) => {
                    let (stdout, stdout_meta) = truncate_with_meta(&output.stdout);
                    let (stderr, stderr_meta) = truncate_with_meta(&output.stderr);
                    ShellResult {
                        task_id: None,
                        status: if output.exit_code == 0 {
                            ShellStatus::Completed
                        } else {
                            ShellStatus::Failed
                        },
                        exit_code: Some(output.exit_code),
                        stdout,
                        stderr,
                        duration_ms: u64::try_from(started.elapsed().as_millis())
                            .unwrap_or(u64::MAX),
                        stdout_len: stdout_meta.original_len,
                        stderr_len: stderr_meta.original_len,
                        stdout_omitted: stdout_meta.omitted,
                        stderr_omitted: stderr_meta.omitted,
                        stdout_truncated: stdout_meta.truncated,
                        stderr_truncated: stderr_meta.truncated,
                        sandboxed: true,
                        sandbox_type: Some("opensandbox".to_string()),
                        sandbox_denied: false,
                    }
                }
                Err(e) => {
                    return Ok(ToolResult::error(format!("Sandbox backend error: {e}")));
                }
            };

            // Build result (reuse the existing output rendering below).
            let stdout_summary = summarize_output(&result.stdout);
            let stderr_summary = summarize_output(&result.stderr);
            let summary = if !stderr_summary.is_empty() {
                stderr_summary.clone()
            } else {
                stdout_summary.clone()
            };
            let output = if result.stdout.is_empty() && result.stderr.is_empty() {
                "(no output)".to_string()
            } else if result.stderr.is_empty() {
                result.stdout.clone()
            } else {
                format!("{}\n\nSTDERR:\n{}", result.stdout, result.stderr)
            };

            let metadata = json!({
                "exit_code": result.exit_code,
                "status": format!("{:?}", result.status),
                "duration_ms": result.duration_ms,
                "sandboxed": true,
                "sandbox_type": "opensandbox",
                "sandbox_denied": false,
                "task_id": result.task_id,
                "stdout_len": result.stdout_len,
                "stderr_len": result.stderr_len,
                "stdout_truncated": result.stdout_truncated,
                "stderr_truncated": result.stderr_truncated,
                "stdout_omitted": result.stdout_omitted,
                "stderr_omitted": result.stderr_omitted,
                "summary": summary,
                "stdout_summary": stdout_summary,
                "stderr_summary": stderr_summary,
                "safety_level": format!("{:?}", safety.level),
                "interactive": false,
                "canceled": false,
                "sandbox_backend": "opensandbox",
            });

            return Ok(ToolResult {
                content: output,
                success: result.status == ShellStatus::Completed,
                metadata: Some(metadata),
            });
        }

        let result = if interactive {
            let mut manager = context
                .shell_manager
                .lock()
                .map_err(|_| ToolError::execution_failed("shell manager lock poisoned"))?;
            manager.execute_interactive_with_policy_env(
                command,
                working_dir.as_deref(),
                timeout_ms,
                policy_override,
                extra_env,
            )
        } else if background {
            let mut manager = context
                .shell_manager
                .lock()
                .map_err(|_| ToolError::execution_failed("shell manager lock poisoned"))?;
            manager.execute_with_options_env(
                command,
                working_dir.as_deref(),
                timeout_ms,
                true,
                stdin_data.as_deref(),
                tty,
                policy_override,
                extra_env,
            )
        } else {
            execute_foreground_via_background(
                context,
                command,
                timeout_ms,
                stdin_data.as_deref(),
                policy_override,
                extra_env,
            )
            .await
        };

        match result {
            Ok(result) => {
                let backgrounded_foreground =
                    !background && !interactive && result.status == ShellStatus::Running;
                if (background || backgrounded_foreground)
                    && let (Some(shell_id), Some(task_id)) = (
                        result.task_id.as_deref(),
                        context.runtime.wire.active_task_id.clone(),
                    )
                    && let Ok(mut manager) = context.shell_manager.lock()
                {
                    let _ = manager.tag_linked_task(shell_id, Some(task_id));
                }

                let was_cancelled = context
                    .cancel_token
                    .as_ref()
                    .is_some_and(|token| token.is_cancelled());
                let task_id_str = result.task_id.clone().unwrap_or_default();
                let stdout_summary = summarize_output(&result.stdout);
                let stderr_summary = summarize_output(&result.stderr);
                let summary = if !stderr_summary.is_empty() {
                    stderr_summary.clone()
                } else {
                    stdout_summary.clone()
                };
                let output = if interactive {
                    format!(
                        "Interactive command completed (exit code: {:?})",
                        result.exit_code
                    )
                } else if result.status == ShellStatus::Completed {
                    if result.stdout.is_empty() && result.stderr.is_empty() {
                        "(no output)".to_string()
                    } else if result.stderr.is_empty() {
                        result.stdout.clone()
                    } else {
                        format!("{}\n\nSTDERR:\n{}", result.stdout, result.stderr)
                    }
                } else if result.status == ShellStatus::Running {
                    if backgrounded_foreground {
                        format!(
                            "Command moved to background: {task_id_str}\n\nPoll with exec_shell_wait or cancel with exec_shell_cancel."
                        )
                    } else {
                        format!("Background task started: {task_id_str}")
                    }
                } else if result.status == ShellStatus::Killed && was_cancelled {
                    format!(
                        "Command canceled; process killed.\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                        result.stdout, result.stderr
                    )
                } else if result.status == ShellStatus::TimedOut {
                    format!(
                        "Command timed out after {timeout_ms}ms; process killed.\n\n{FOREGROUND_TIMEOUT_RECOVERY_HINT}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                        result.stdout, result.stderr
                    )
                } else {
                    format!(
                        "Command failed (exit code: {:?})\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                        result.exit_code, result.stdout, result.stderr
                    )
                };

                let mut metadata = json!({
                    "exit_code": result.exit_code,
                    "status": format!("{:?}", result.status),
                    "duration_ms": result.duration_ms,
                    "sandboxed": result.sandboxed,
                    "sandbox_type": result.sandbox_type,
                    "sandbox_denied": result.sandbox_denied,
                    "task_id": result.task_id,
                    "stdout_len": result.stdout_len,
                    "stderr_len": result.stderr_len,
                    "stdout_truncated": result.stdout_truncated,
                    "stderr_truncated": result.stderr_truncated,
                    "stdout_omitted": result.stdout_omitted,
                    "stderr_omitted": result.stderr_omitted,
                    "summary": summary,
                    "stdout_summary": stdout_summary,
                    "stderr_summary": stderr_summary,
                    "safety_level": format!("{:?}", safety.level),
                    "interactive": interactive,
                    "canceled": was_cancelled,
                    "execpolicy": execpolicy_decision.as_ref().map(|decision| match decision {
                        ExecPolicyDecision::Allow => json!({
                            "decision": "allow",
                        }),
                        ExecPolicyDecision::Deny(reason) => json!({
                            "decision": "deny",
                            "reason": reason,
                        }),
                        ExecPolicyDecision::AskUser(reason) => json!({
                            "decision": "ask_user",
                            "reason": reason,
                        }),
                    }),
                });
                metadata["backgrounded"] = json!(background || backgrounded_foreground);
                if result.status == ShellStatus::TimedOut && !background && !interactive {
                    metadata["foreground_timeout_recovery"] = json!({
                        "process_killed": true,
                        "hint": FOREGROUND_TIMEOUT_RECOVERY_HINT,
                        "recommended_tools": [
                            "task_shell_start",
                            "task_shell_wait",
                            "exec_shell",
                            "exec_shell_wait"
                        ],
                        "exec_shell_background": true,
                        "poll_with": ["task_shell_wait", "exec_shell_wait"]
                    });
                }

                Ok(ToolResult {
                    content: output,
                    success: result.status == ShellStatus::Completed
                        || result.status == ShellStatus::Running,
                    metadata: Some(metadata),
                })
            }
            Err(e) => Ok(ToolResult::error(format!("Shell execution failed: {e}"))),
        }
    }
}

pub struct ShellWaitTool {
    name: &'static str,
}

impl ShellWaitTool {
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }
}

pub struct ShellInteractTool {
    name: &'static str,
}

impl ShellInteractTool {
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }
}

fn required_task_id(input: &serde_json::Value) -> Result<&str, ToolError> {
    input
        .get("task_id")
        .or_else(|| input.get("id"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ToolError::missing_field("task_id"))
}

fn build_shell_delta_tool_result(delta: ShellDeltaResult) -> ToolResult {
    let result = delta.result;
    let stdout_summary = summarize_output(&result.stdout);
    let stderr_summary = summarize_output(&result.stderr);
    let summary = if !stderr_summary.is_empty() {
        stderr_summary.clone()
    } else {
        stdout_summary.clone()
    };

    let output = if result.stdout.is_empty() && result.stderr.is_empty() {
        match result.status {
            ShellStatus::Running => "Background task running (no new output).".to_string(),
            ShellStatus::Completed => "(no new output)".to_string(),
            ShellStatus::Failed => format!("Command failed (exit code: {:?})", result.exit_code),
            ShellStatus::TimedOut => "Command timed out (no new output).".to_string(),
            ShellStatus::Killed => "Command killed (no new output).".to_string(),
        }
    } else if result.stderr.is_empty() {
        result.stdout.clone()
    } else {
        format!("{}\n\nSTDERR:\n{}", result.stdout, result.stderr)
    };

    ToolResult {
        content: output,
        success: matches!(result.status, ShellStatus::Completed | ShellStatus::Running),
        metadata: Some(json!({
            "exit_code": result.exit_code,
            "status": format!("{:?}", result.status),
            "duration_ms": result.duration_ms,
            "sandboxed": result.sandboxed,
            "sandbox_type": result.sandbox_type,
            "sandbox_denied": result.sandbox_denied,
            "task_id": result.task_id,
            "stdout_len": result.stdout_len,
            "stderr_len": result.stderr_len,
            "stdout_truncated": result.stdout_truncated,
            "stderr_truncated": result.stderr_truncated,
            "stdout_omitted": result.stdout_omitted,
            "stderr_omitted": result.stderr_omitted,
            "stdout_total_len": delta.stdout_total_len,
            "stderr_total_len": delta.stderr_total_len,
            "summary": summary,
            "stdout_summary": stdout_summary,
            "stderr_summary": stderr_summary,
            "stream_delta": true,
        })),
    }
}

async fn wait_for_shell_delta_cancellable(
    context: &ToolContext,
    task_id: &str,
    timeout_ms: u64,
) -> Result<(ShellDeltaResult, bool), ToolError> {
    let timeout_ms = timeout_ms.clamp(1000, 600_000);
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut stdout_accum = String::new();
    let mut stderr_accum = String::new();

    let (result, stdout_total_len, stderr_total_len) = loop {
        if context
            .cancel_token
            .as_ref()
            .is_some_and(|token| token.is_cancelled())
        {
            let mut manager = context
                .shell_manager
                .lock()
                .map_err(|_| ToolError::execution_failed("shell manager lock poisoned"))?;
            let delta = manager
                .get_output_delta(task_id, false, 0)
                .map_err(|err| ToolError::execution_failed(err.to_string()))?;
            append_shell_delta_output(&mut stdout_accum, &mut stderr_accum, &delta.result);
            emit_shell_delta_streams(context, &delta.result);
            return Ok((
                shell_delta_with_accumulated_output(
                    delta.result,
                    &stdout_accum,
                    &stderr_accum,
                    delta.stdout_total_len,
                    delta.stderr_total_len,
                ),
                true,
            ));
        }

        let delta = {
            let mut manager = context
                .shell_manager
                .lock()
                .map_err(|_| ToolError::execution_failed("shell manager lock poisoned"))?;
            manager
                .get_output_delta(task_id, false, 0)
                .map_err(|err| ToolError::execution_failed(err.to_string()))?
        };

        let stdout_total_len = delta.stdout_total_len;
        let stderr_total_len = delta.stderr_total_len;
        append_shell_delta_output(&mut stdout_accum, &mut stderr_accum, &delta.result);
        emit_shell_delta_streams(context, &delta.result);

        let status = delta.result.status.clone();
        if status != ShellStatus::Running || Instant::now() >= deadline {
            break (delta.result, stdout_total_len, stderr_total_len);
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    };

    Ok((
        shell_delta_with_accumulated_output(
            result,
            &stdout_accum,
            &stderr_accum,
            stdout_total_len,
            stderr_total_len,
        ),
        false,
    ))
}

fn append_shell_delta_output(
    stdout_accum: &mut String,
    stderr_accum: &mut String,
    result: &ShellResult,
) {
    if !result.stdout.is_empty() {
        stdout_accum.push_str(&result.stdout);
    }
    if !result.stderr.is_empty() {
        stderr_accum.push_str(&result.stderr);
    }
}

fn shell_delta_with_accumulated_output(
    mut result: ShellResult,
    stdout_accum: &str,
    stderr_accum: &str,
    stdout_total_len: usize,
    stderr_total_len: usize,
) -> ShellDeltaResult {
    let (stdout, stdout_meta) = truncate_with_meta(stdout_accum);
    let (stderr, stderr_meta) = truncate_with_meta(stderr_accum);
    result.stdout = stdout;
    result.stderr = stderr;
    result.stdout_len = stdout_meta.original_len;
    result.stderr_len = stderr_meta.original_len;
    result.stdout_omitted = stdout_meta.omitted;
    result.stderr_omitted = stderr_meta.omitted;
    result.stdout_truncated = stdout_meta.truncated;
    result.stderr_truncated = stderr_meta.truncated;

    ShellDeltaResult {
        result,
        stdout_total_len,
        stderr_total_len,
    }
}

pub struct ShellCancelTool;

#[async_trait]
impl ToolSpec for ShellCancelTool {
    fn name(&self) -> &'static str {
        "exec_shell_cancel"
    }

    fn description(&self) -> &'static str {
        "Cancel a running background shell task by task_id, or cancel all running background shell tasks with all=true."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID returned by exec_shell or task_shell_start"
                },
                "id": {
                    "type": "string",
                    "description": "Alias for task_id"
                },
                "all": {
                    "type": "boolean",
                    "description": "Cancel all currently running background shell tasks"
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let cancel_all = optional_bool(&input, "all", false);
        let mut manager = context
            .shell_manager
            .lock()
            .map_err(|_| ToolError::execution_failed("shell manager lock poisoned"))?;

        if cancel_all {
            let results = manager
                .kill_running()
                .map_err(|err| ToolError::execution_failed(err.to_string()))?;
            if results.is_empty() {
                return Ok(ToolResult {
                    content: "No running background shell jobs.".to_string(),
                    success: true,
                    metadata: Some(json!({
                        "status": "Noop",
                        "canceled": 0,
                        "task_ids": [],
                    })),
                });
            }

            let task_ids = results
                .iter()
                .filter_map(|result| result.task_id.clone())
                .collect::<Vec<_>>();
            return Ok(ToolResult {
                content: format!(
                    "Canceled {} background shell job{}: {}",
                    task_ids.len(),
                    if task_ids.len() == 1 { "" } else { "s" },
                    task_ids.join(", ")
                ),
                success: true,
                metadata: Some(json!({
                    "status": "Killed",
                    "canceled": task_ids.len(),
                    "task_ids": task_ids,
                })),
            });
        }

        let task_id = required_task_id(&input)?;
        let result = manager
            .kill(task_id)
            .map_err(|err| ToolError::execution_failed(err.to_string()))?;
        let task_id = result
            .task_id
            .clone()
            .unwrap_or_else(|| task_id.to_string());
        Ok(ToolResult {
            content: format!("Canceled background shell job: {task_id}"),
            success: true,
            metadata: Some(json!({
                "status": format!("{:?}", result.status),
                "task_id": task_id,
                "exit_code": result.exit_code,
                "duration_ms": result.duration_ms,
            })),
        })
    }
}

#[async_trait]
impl ToolSpec for ShellWaitTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Wait for a background shell task and return incremental output. Turn cancellation stops waiting but leaves the background task running."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID returned by exec_shell"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default: 5000)"
                },
                "wait": {
                    "type": "boolean",
                    "description": "Wait for completion before returning (default: true)"
                }
            },
            "required": ["task_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let task_id = required_task_id(&input)?;
        let wait = optional_bool(&input, "wait", true);
        let timeout_ms = optional_u64(&input, "timeout_ms", 5_000);

        let (delta, wait_canceled) = if wait {
            wait_for_shell_delta_cancellable(context, task_id, timeout_ms).await?
        } else {
            let mut manager = context
                .shell_manager
                .lock()
                .map_err(|_| ToolError::execution_failed("shell manager lock poisoned"))?;
            let delta = manager
                .get_output_delta(task_id, false, timeout_ms)
                .map_err(|err| ToolError::execution_failed(err.to_string()))?;
            emit_shell_delta_streams(context, &delta.result);
            (delta, false)
        };

        let status = delta.result.status.clone();
        let mut result = build_shell_delta_tool_result(delta);
        if wait_canceled {
            if matches!(status, ShellStatus::Running) {
                result.content = format!(
                    "Wait canceled; background shell task {task_id} is still running.\n\n{}",
                    result.content
                );
            }
            if let Some(metadata) = result.metadata.as_mut()
                && let Some(object) = metadata.as_object_mut()
            {
                object.insert("wait_canceled".to_string(), json!(true));
            }
        }

        Ok(result)
    }
}

#[async_trait]
impl ToolSpec for ShellInteractTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Send input to a background shell task and return incremental output."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID returned by exec_shell"
                },
                "input": {
                    "type": "string",
                    "description": "Input to send to the task's stdin"
                },
                "stdin": {
                    "type": "string",
                    "description": "Alias for input"
                },
                "data": {
                    "type": "string",
                    "description": "Alias for input"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Wait for output after sending input (default: 1000)"
                },
                "close_stdin": {
                    "type": "boolean",
                    "description": "Close stdin after sending input"
                }
            },
            "required": ["task_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ExecutesCode]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let task_id = required_task_id(&input)?;
        let close_stdin = optional_bool(&input, "close_stdin", false);
        let timeout_ms = optional_u64(&input, "timeout_ms", 1_000);
        let interaction_input = input
            .get("input")
            .or_else(|| input.get("stdin"))
            .or_else(|| input.get("data"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");

        {
            let mut manager = context
                .shell_manager
                .lock()
                .map_err(|_| ToolError::execution_failed("shell manager lock poisoned"))?;
            if !interaction_input.is_empty() || close_stdin {
                manager
                    .write_stdin(task_id, interaction_input, close_stdin)
                    .map_err(|err| ToolError::execution_failed(err.to_string()))?;
            }
        }

        let mut elapsed = 0u64;
        loop {
            if context
                .cancel_token
                .as_ref()
                .is_some_and(|token| token.is_cancelled())
            {
                let mut manager = context
                    .shell_manager
                    .lock()
                    .map_err(|_| ToolError::execution_failed("shell manager lock poisoned"))?;
                let delta = manager
                    .get_output_delta(task_id, false, 0)
                    .map_err(|err| ToolError::execution_failed(err.to_string()))?;
                emit_shell_delta_streams(context, &delta.result);
                let mut result = build_shell_delta_tool_result(delta);
                if let Some(metadata) = result.metadata.as_mut()
                    && let Some(object) = metadata.as_object_mut()
                {
                    object.insert("wait_canceled".to_string(), json!(true));
                }
                return Ok(result);
            }

            let delta = {
                let mut manager = context
                    .shell_manager
                    .lock()
                    .map_err(|_| ToolError::execution_failed("shell manager lock poisoned"))?;
                manager
                    .get_output_delta(task_id, false, 0)
                    .map_err(|err| ToolError::execution_failed(err.to_string()))?
            };

            if !delta.result.stdout.is_empty()
                || !delta.result.stderr.is_empty()
                || delta.result.status != ShellStatus::Running
                || elapsed >= timeout_ms
            {
                emit_shell_delta_streams(context, &delta.result);
                return Ok(build_shell_delta_tool_result(delta));
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
            elapsed = elapsed.saturating_add(50);
        }
    }
}

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
