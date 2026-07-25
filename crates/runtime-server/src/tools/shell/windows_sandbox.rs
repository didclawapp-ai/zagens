//! Windows enforced sandbox spawn helpers for `ShellManager`.

#[cfg(windows)]
mod imp {
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result};

    use crate::sandbox::{ExecEnv, SandboxManager};
    use crate::tools::shell::process::{ShellChild, StdinWriter, spawn_reader_thread_from_handle};
    use crate::tools::shell::sandbox_meta;
    use crate::tools::shell::types::{ShellResult, ShellStatus};
    use crate::tools::shell_output::truncate_shell_streams_with_spill;

    type SpawnBackgroundHandles = (
        ShellChild,
        Option<StdinWriter>,
        Option<JoinHandle<()>>,
        Option<JoinHandle<()>>,
    );

    pub fn execute_sync(
        original_command: &str,
        exec_env: &ExecEnv,
        timeout_ms: u64,
        stdin_data: Option<&str>,
        workspace: &std::path::Path,
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

        let output = match zagens_windows_sandbox::spawn_sync(plan, stdin_data, Some(timeout)) {
            Ok(output) => output,
            Err(err) => {
                // PR-2.13: a spawn-level denial (CreateProcessAsUserW /
                // CreateProcessWithLogonW / runner) carries a structured Win32
                // code. Surface it as a denied ShellResult so the elevation
                // flow can react, instead of an opaque tool error.
                if let Some(code) = zagens_windows_sandbox::extract_spawn_denial_code(&err) {
                    return Ok(denied_spawn_result(
                        exec_env,
                        started,
                        code,
                        &format!("{err:#}"),
                    ));
                }
                return Err(err)
                    .with_context(|| format!("Windows sandbox spawn failed: {original_command}"));
            }
        };

        let timed_out = started.elapsed() >= timeout && output.exit_code != 0;
        let exit_code = i32::try_from(output.exit_code).unwrap_or(-1);
        let sandbox_denied = SandboxManager::was_denied(sandbox_type, exit_code, &output.stderr);
        let (stdout, stdout_meta, stderr, stderr_meta, spill) =
            truncate_shell_streams_with_spill(workspace, &output.stdout, &output.stderr);

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
            full_output_spill_path: spill.map(|s| s.read_path_hint),
            sandboxed,
            sandbox_enforced,
            sandbox_type: if sandboxed {
                Some(sandbox_type.to_string())
            } else {
                None
            },
            sandbox_denied,
            sandbox_denial_code: None,
            windows_sandbox_mode: sandbox_meta::windows_sandbox_mode_from_env(exec_env),
        })
    }

    /// ShellResult for a spawn that the sandbox denied before the child ran.
    fn denied_spawn_result(
        exec_env: &ExecEnv,
        started: Instant,
        win32_code: u32,
        message: &str,
    ) -> ShellResult {
        let (stderr, stderr_meta) = crate::tools::shell_output::truncate_with_meta(message);
        ShellResult {
            task_id: None,
            status: ShellStatus::Failed,
            exit_code: Some(i32::try_from(win32_code).unwrap_or(-1)),
            stdout: String::new(),
            stderr,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            stdout_len: 0,
            stderr_len: stderr_meta.original_len,
            stdout_omitted: 0,
            stderr_omitted: stderr_meta.omitted,
            stdout_truncated: false,
            stderr_truncated: stderr_meta.truncated,
            full_output_spill_path: None,
            sandboxed: exec_env.is_sandboxed(),
            sandbox_enforced: exec_env.is_enforced(),
            sandbox_type: Some(exec_env.sandbox_type.to_string()),
            sandbox_denied: true,
            sandbox_denial_code: Some(win32_code),
            windows_sandbox_mode: sandbox_meta::windows_sandbox_mode_from_env(exec_env),
        }
    }

    pub fn spawn_background(
        exec_env: &ExecEnv,
        stdout_buffer: Arc<Mutex<Vec<u8>>>,
        stderr_buffer: Arc<Mutex<Vec<u8>>>,
        stdin_data: Option<&str>,
        tty: bool,
    ) -> Result<SpawnBackgroundHandles> {
        let mut plan = exec_env
            .windows_plan
            .as_ref()
            .context("missing WindowsExecPlan for enforced spawn")?
            .clone();
        if tty {
            plan.tty = true;
        }

        if matches!(
            plan.mode,
            zagens_windows_sandbox::WindowsSandboxMode::Elevated
        ) {
            return spawn_background_elevated(&plan, stdout_buffer, stderr_buffer, stdin_data);
        }

        let mut managed = zagens_windows_sandbox::spawn(
            &plan,
            zagens_windows_sandbox::SpawnStdio {
                capture_stdout: true,
                capture_stderr: !tty,
                stdin_open: true,
                stdin_data: stdin_data.map(str::to_string),
            },
        )?;

        let (stdout_handle, stderr_handle) = managed.detach_output_readers();
        let stdout_thread = spawn_reader_thread_from_handle(stdout_handle, stdout_buffer);
        let stderr_thread = if stderr_handle != 0 {
            Some(spawn_reader_thread_from_handle(
                stderr_handle,
                stderr_buffer,
            ))
        } else {
            None
        };

        Ok((
            ShellChild::WindowsSandbox(managed),
            None,
            Some(stdout_thread),
            stderr_thread,
        ))
    }

    /// Elevated background spawn: child output arrives as runner IPC frames,
    /// pumped into the shared buffers by a single decoder thread.
    fn spawn_background_elevated(
        plan: &zagens_windows_sandbox::WindowsExecPlan,
        stdout_buffer: Arc<Mutex<Vec<u8>>>,
        stderr_buffer: Arc<Mutex<Vec<u8>>>,
        stdin_data: Option<&str>,
    ) -> Result<SpawnBackgroundHandles> {
        let mut child = zagens_windows_sandbox::spawn_background_elevated(plan, stdin_data)?;
        let pump = child.start_output_pump(
            move |chunk| {
                if let Ok(mut guard) = stdout_buffer.lock() {
                    guard.extend_from_slice(chunk);
                }
            },
            move |chunk| {
                if let Ok(mut guard) = stderr_buffer.lock() {
                    guard.extend_from_slice(chunk);
                }
            },
        )?;

        Ok((
            ShellChild::ElevatedWindowsSandbox(child),
            None,
            Some(pump),
            None,
        ))
    }
}

#[cfg(windows)]
pub use imp::{execute_sync, spawn_background};
