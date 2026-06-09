//! Windows enforced sandbox spawn helpers for `ShellManager`.

#[cfg(windows)]
mod imp {
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result};

    use crate::sandbox::{ExecEnv, SandboxManager};
    use crate::tools::shell::process::{ShellChild, StdinWriter, spawn_reader_thread_from_handle};
    use crate::tools::shell::types::{ShellResult, ShellStatus};
    use crate::tools::shell_output::truncate_with_meta;

    pub fn execute_sync(
        original_command: &str,
        exec_env: &ExecEnv,
        timeout_ms: u64,
        stdin_data: Option<&str>,
    ) -> Result<ShellResult> {
        let plan = exec_env
            .windows_plan
            .as_ref()
            .context("missing WindowsExecPlan for enforced spawn")?;
        let started = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        let sandbox_type = exec_env.sandbox_type;
        let sandboxed = exec_env.is_sandboxed();
        let sandbox_enforced = exec_env.is_enforced();

        let output = zagens_windows_sandbox::spawn_sync(plan, stdin_data, Some(timeout))
            .with_context(|| format!("Windows sandbox spawn failed: {original_command}"))?;

        let timed_out = started.elapsed() >= timeout && output.exit_code != 0;
        let exit_code = i32::try_from(output.exit_code).unwrap_or(-1);
        let sandbox_denied = SandboxManager::was_denied(sandbox_type, exit_code, &output.stderr);
        let (stdout, stdout_meta) = truncate_with_meta(&output.stdout);
        let (stderr, stderr_meta) = truncate_with_meta(&output.stderr);

        Ok(ShellResult {
            task_id: None,
            status: if timed_out {
                ShellStatus::TimedOut
            } else if output.exit_code == 0 {
                ShellStatus::Completed
            } else {
                ShellStatus::Failed
            },
            exit_code: Some(exit_code),
            stdout,
            stderr,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            stdout_len: stdout_meta.original_len,
            stderr_len: stderr_meta.original_len,
            stdout_omitted: stdout_meta.omitted,
            stderr_omitted: stderr_meta.omitted,
            stdout_truncated: stdout_meta.truncated,
            stderr_truncated: stderr_meta.truncated,
            sandboxed,
            sandbox_enforced,
            sandbox_type: if sandboxed {
                Some(sandbox_type.to_string())
            } else {
                None
            },
            sandbox_denied,
        })
    }

    pub fn spawn_background(
        exec_env: &ExecEnv,
        stdout_buffer: Arc<Mutex<Vec<u8>>>,
        stderr_buffer: Arc<Mutex<Vec<u8>>>,
        stdin_data: Option<&str>,
    ) -> Result<(
        ShellChild,
        Option<StdinWriter>,
        Option<JoinHandle<()>>,
        Option<JoinHandle<()>>,
    )> {
        let plan = exec_env
            .windows_plan
            .as_ref()
            .context("missing WindowsExecPlan for enforced spawn")?;

        let mut managed = zagens_windows_sandbox::spawn(
            plan,
            zagens_windows_sandbox::SpawnStdio {
                capture_stdout: true,
                capture_stderr: true,
                stdin_data: stdin_data.map(str::to_string),
            },
        )?;

        let (stdout_handle, stderr_handle) = managed.detach_output_readers();
        let stdout_thread = spawn_reader_thread_from_handle(stdout_handle, stdout_buffer);
        let stderr_thread = spawn_reader_thread_from_handle(stderr_handle, stderr_buffer);

        Ok((
            ShellChild::WindowsSandbox(managed),
            None,
            Some(stdout_thread),
            Some(stderr_thread),
        ))
    }
}

#[cfg(windows)]
pub use imp::{execute_sync, spawn_background};
