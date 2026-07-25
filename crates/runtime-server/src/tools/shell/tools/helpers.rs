//! Shared helpers for shell ToolSpec implementations.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

use super::super::types::{ShellDeltaResult, ShellResult, ShellStatus};
use crate::sandbox::SandboxPolicy as ExecutionSandboxPolicy;
use crate::tools::shell_output::{append_shell_spill_note, summarize_output};
use crate::tools::spec::{ToolContext, ToolError, ToolResult};
use serde_json::json;

const FOREGROUND_TIMEOUT_RECOVERY_HINT: &str = "Foreground exec_shell is for bounded commands. \
The timed-out process was killed; rerun long work with task_shell_start or exec_shell with \
background: true, then poll with task_shell_wait or exec_shell_wait.";

pub(super) fn emit_shell_snapshot_stream(
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

pub(super) fn emit_shell_delta_streams(context: &ToolContext, result: &ShellResult) {
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

pub(super) async fn execute_foreground_via_background(
    context: &ToolContext,
    command: &str,
    working_dir: Option<&str>,
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
            working_dir,
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
pub(super) fn required_task_id(input: &serde_json::Value) -> Result<&str, ToolError> {
    input
        .get("task_id")
        .or_else(|| input.get("id"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ToolError::missing_field("task_id"))
}

pub(super) fn shell_evidence(
    exit_code: Option<i32>,
    status: &ShellStatus,
    stdout_truncated: bool,
    stderr_truncated: bool,
) -> zagens_tools::EvidenceEnvelope {
    use zagens_tools::{EvidenceEnvelope, UncertaintyKind};
    let uncertainty = if stdout_truncated || stderr_truncated {
        UncertaintyKind::Truncated
    } else if matches!(
        status,
        ShellStatus::Failed | ShellStatus::TimedOut | ShellStatus::Killed
    ) {
        UncertaintyKind::Partial
    } else {
        UncertaintyKind::None
    };
    let mut evidence = EvidenceEnvelope::new()
        .with_fact("status", format!("{status:?}"))
        .with_uncertainty(uncertainty);
    if let Some(code) = exit_code {
        evidence = evidence.with_fact("exit_code", code.to_string());
    }
    evidence
}

pub(super) fn build_shell_delta_tool_result(delta: ShellDeltaResult) -> ToolResult {
    let result = delta.result;
    let stdout_summary = summarize_output(&result.stdout);
    let stderr_summary = summarize_output(&result.stderr);
    let summary = if !stderr_summary.is_empty() {
        stderr_summary.clone()
    } else {
        stdout_summary.clone()
    };

    let mut output = if result.stdout.is_empty() && result.stderr.is_empty() {
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
    append_shell_spill_note(&mut output, result.full_output_spill_path.as_deref());

    let evidence = shell_evidence(
        result.exit_code,
        &result.status,
        result.stdout_truncated,
        result.stderr_truncated,
    );

    ToolResult {
        content: output,
        success: matches!(result.status, ShellStatus::Completed | ShellStatus::Running),
        metadata: Some(json!({
            "exit_code": result.exit_code,
            "status": format!("{:?}", result.status),
            "duration_ms": result.duration_ms,
            "sandboxed": result.sandboxed,
            "sandbox_enforced": result.sandbox_enforced,
            "sandbox_type": result.sandbox_type,
            "sandbox_denied": result.sandbox_denied,
            "sandbox_denial_code": result.sandbox_denial_code,
            "windows_sandbox_mode": result.windows_sandbox_mode,
            "task_id": result.task_id,
            "stdout_len": result.stdout_len,
            "stderr_len": result.stderr_len,
            "stdout_truncated": result.stdout_truncated,
            "stderr_truncated": result.stderr_truncated,
            "full_output_spill_path": result.full_output_spill_path,
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
    .with_evidence(evidence)
}

pub(super) async fn wait_for_shell_delta_cancellable(
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
                    &context.workspace,
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
            &context.workspace,
        ),
        false,
    ))
}

pub(super) fn append_shell_delta_output(
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

pub(super) fn shell_delta_with_accumulated_output(
    mut result: ShellResult,
    stdout_accum: &str,
    stderr_accum: &str,
    stdout_total_len: usize,
    stderr_total_len: usize,
    workspace: &std::path::Path,
) -> ShellDeltaResult {
    crate::tools::shell_output::assign_truncated_shell_streams(
        &mut result,
        workspace,
        stdout_accum,
        stderr_accum,
    );

    ShellDeltaResult {
        result,
        stdout_total_len,
        stderr_total_len,
    }
}
