//! Background shell process tracking and low-level spawn helpers.

use anyhow::{Context, Result, anyhow};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use super::types::{ShellJobDetail, ShellJobSnapshot, ShellResult, ShellStatus};
use crate::sandbox::{ExecEnv, SandboxManager, SandboxType};
use crate::tools::shell_output::truncate_with_meta;

pub(in crate::tools::shell) fn prepend_sandbox_enforcement_warning(
    exec_env: &ExecEnv,
    stderr: &mut String,
) {
    if let Some(warning) = exec_env.sandbox_enforcement_warning() {
        let prefix = format!("[sandbox] {warning}\n");
        if stderr.is_empty() {
            *stderr = prefix;
        } else {
            stderr.insert_str(0, &prefix);
        }
    }
}
pub(in crate::tools::shell) enum ShellChild {
    Process(Child),
    Pty(Box<dyn portable_pty::Child + Send>),
}

#[cfg(unix)]
pub(in crate::tools::shell) fn kill_child_process_group(child: &mut Child) -> std::io::Result<()> {
    let pgid = child.id() as libc::pid_t;
    if pgid <= 0 {
        return child.kill();
    }

    let result = unsafe { libc::kill(-pgid, libc::SIGKILL) };
    if result == 0 {
        Ok(())
    } else {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            child.kill()
        }
    }
}

/// Configure parent-death signaling so shell-spawned children are reaped when
/// the TUI dies abnormally (#421). On Linux this installs
/// `PR_SET_PDEATHSIG(SIGTERM)` via `pre_exec` — the kernel then sends SIGTERM
/// to the child the moment the parent process exits, even on SIGKILL of the
/// TUI. The cancellation path already SIGKILLs the whole process group, so
/// this only fires when the parent dies without running its drop / cleanup
/// code (panic during shutdown, OOM, hardware crash, etc.).
///
/// On macOS / Windows there's no kernel equivalent. The existing graceful
/// path (`kill_child_process_group` from the cancellation token) still
/// handles normal shutdown; abnormal exit can leak children — tracked as a
/// follow-up watchdog item per the original issue's acceptance criteria.
#[cfg(target_os = "linux")]
pub(in crate::tools::shell) fn install_parent_death_signal(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: `pre_exec` runs in the child between fork and exec. The closure
    // only calls `libc::prctl` with stack-allocated constant arguments and
    // does not touch heap memory or the parent's locks. Both requirements
    // (async-signal-safe + no allocation in the post-fork window) are met.
    unsafe {
        cmd.pre_exec(|| {
            let result = libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM, 0, 0, 0);
            if result == -1 {
                // Surface the errno but do not abort the spawn — the child
                // will simply lose the parent-death cleanup safety net.
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
}

#[cfg(not(target_os = "linux"))]
pub(in crate::tools::shell) fn install_parent_death_signal(_cmd: &mut Command) {
    // No kernel-level equivalent on macOS / Windows. The cooperative
    // cancellation + process_group SIGKILL path covers normal shutdown;
    // abnormal exit (panic without unwind, SIGKILL of the TUI) can still
    // leak children on those platforms — tracked as a follow-up.
}

#[derive(Clone, Copy, Debug)]
struct ShellExitStatus {
    code: Option<i32>,
    success: bool,
}

impl ShellExitStatus {
    fn from_std(status: std::process::ExitStatus) -> Self {
        Self {
            code: status.code(),
            success: status.success(),
        }
    }

    fn from_pty(status: portable_pty::ExitStatus) -> Self {
        let code = i32::try_from(status.exit_code()).unwrap_or(i32::MAX);
        Self {
            code: Some(code),
            success: status.success(),
        }
    }
}

impl ShellChild {
    fn try_wait(&mut self) -> std::io::Result<Option<ShellExitStatus>> {
        match self {
            ShellChild::Process(child) => child
                .try_wait()
                .map(|status| status.map(ShellExitStatus::from_std)),
            ShellChild::Pty(child) => child
                .try_wait()
                .map(|status| status.map(ShellExitStatus::from_pty)),
        }
    }

    fn wait(&mut self) -> std::io::Result<ShellExitStatus> {
        match self {
            ShellChild::Process(child) => child.wait().map(ShellExitStatus::from_std),
            ShellChild::Pty(child) => child.wait().map(ShellExitStatus::from_pty),
        }
    }

    fn kill(&mut self) -> std::io::Result<()> {
        match self {
            #[cfg(unix)]
            ShellChild::Process(child) => kill_child_process_group(child),
            #[cfg(not(unix))]
            ShellChild::Process(child) => child.kill(),
            ShellChild::Pty(child) => child.kill(),
        }
    }
}

pub(in crate::tools::shell) enum StdinWriter {
    Pipe(ChildStdin),
    Pty(Box<dyn Write + Send>),
}

impl StdinWriter {
    fn write_all(&mut self, data: &[u8]) -> std::io::Result<()> {
        match self {
            StdinWriter::Pipe(stdin) => stdin.write_all(data),
            StdinWriter::Pty(writer) => writer.write_all(data),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            StdinWriter::Pipe(stdin) => stdin.flush(),
            StdinWriter::Pty(writer) => writer.flush(),
        }
    }
}

#[cfg(not(unix))]
pub(in crate::tools::shell) fn kill_child_process_group(child: &mut Child) -> std::io::Result<()> {
    child.kill()
}

pub(in crate::tools::shell) fn spawn_reader_thread<R: Read + Send + 'static>(
    mut reader: R,
    buffer: Arc<Mutex<Vec<u8>>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    if let Ok(mut guard) = buffer.lock() {
                        guard.extend_from_slice(&chunk[..n]);
                    }
                }
                Err(_) => break,
            }
        }
    })
}

/// A background shell process being tracked
pub struct BackgroundShell {
    pub id: String,
    pub command: String,
    pub working_dir: PathBuf,
    pub status: ShellStatus,
    pub exit_code: Option<i32>,
    pub started_at: Instant,
    pub sandbox_type: SandboxType,
    pub linked_task_id: Option<String>,
    pub(in crate::tools::shell) stdout_buffer: Arc<Mutex<Vec<u8>>>,
    pub(in crate::tools::shell) stderr_buffer: Option<Arc<Mutex<Vec<u8>>>>,
    pub(in crate::tools::shell) stdout_cursor: usize,
    pub(in crate::tools::shell) stderr_cursor: usize,
    pub(in crate::tools::shell) stdin: Option<StdinWriter>,
    pub(in crate::tools::shell) child: Option<ShellChild>,
    pub(in crate::tools::shell) stdout_thread: Option<std::thread::JoinHandle<()>>,
    pub(in crate::tools::shell) stderr_thread: Option<std::thread::JoinHandle<()>>,
}

impl BackgroundShell {
    /// Check if the process has completed and update status
    pub(in crate::tools::shell) fn poll(&mut self) -> bool {
        if self.status != ShellStatus::Running {
            return true;
        }

        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.exit_code = status.code;
                    self.status = if status.success {
                        ShellStatus::Completed
                    } else {
                        ShellStatus::Failed
                    };
                    self.collect_output();
                    true
                }
                Ok(None) => false, // Still running
                Err(_) => {
                    self.status = ShellStatus::Failed;
                    self.collect_output();
                    true
                }
            }
        } else {
            true
        }
    }

    /// Collect output from the background threads
    fn collect_output(&mut self) {
        if let Some(handle) = self.stdout_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.stderr_thread.take() {
            let _ = handle.join();
        }
        self.stdin = None;
        self.child = None;
    }

    pub(in crate::tools::shell) fn write_stdin(&mut self, input: &str, close: bool) -> Result<()> {
        if let Some(stdin) = self.stdin.as_mut() {
            if !input.is_empty() {
                stdin
                    .write_all(input.as_bytes())
                    .context("Failed to write to stdin")?;
                stdin.flush().ok();
            }
            if close {
                self.stdin = None;
            }
            return Ok(());
        }

        if input.is_empty() && close {
            return Ok(());
        }

        Err(anyhow!("stdin is not available for task {}", self.id))
    }

    fn full_output(&self) -> (String, String, usize, usize) {
        let stdout_bytes = self
            .stdout_buffer
            .lock()
            .map(|data| data.clone())
            .unwrap_or_default();
        let stderr_bytes = self
            .stderr_buffer
            .as_ref()
            .and_then(|buffer| buffer.lock().ok().map(|data| data.clone()))
            .unwrap_or_default();

        let stdout_len = stdout_bytes.len();
        let stderr_len = stderr_bytes.len();

        (
            String::from_utf8_lossy(&stdout_bytes).to_string(),
            String::from_utf8_lossy(&stderr_bytes).to_string(),
            stdout_len,
            stderr_len,
        )
    }

    pub(in crate::tools::shell) fn take_delta(
        &mut self,
    ) -> (String, String, usize, usize, usize, usize) {
        let (stdout_delta, stdout_total) =
            take_delta_from_buffer(&self.stdout_buffer, &mut self.stdout_cursor);
        let (stderr_delta, stderr_total) = if let Some(buffer) = self.stderr_buffer.as_ref() {
            take_delta_from_buffer(buffer, &mut self.stderr_cursor)
        } else {
            (Vec::new(), 0)
        };

        let stdout_delta_len = stdout_delta.len();
        let stderr_delta_len = stderr_delta.len();

        (
            String::from_utf8_lossy(&stdout_delta).to_string(),
            String::from_utf8_lossy(&stderr_delta).to_string(),
            stdout_delta_len,
            stderr_delta_len,
            stdout_total,
            stderr_total,
        )
    }

    pub(in crate::tools::shell) fn sandbox_denied(&self) -> bool {
        if matches!(self.status, ShellStatus::Running) {
            return false;
        }
        let (_, stderr_full, _, _) = self.full_output();
        SandboxManager::was_denied(
            self.sandbox_type,
            self.exit_code.unwrap_or(-1),
            &stderr_full,
        )
    }

    /// Kill the process
    pub(in crate::tools::shell) fn kill(&mut self) -> Result<()> {
        if let Some(ref mut child) = self.child {
            child.kill().context("Failed to kill process")?;
            let _ = child.wait();
        }
        self.status = ShellStatus::Killed;
        self.collect_output();
        Ok(())
    }

    /// Get a snapshot of the current state
    #[allow(dead_code)]
    pub fn snapshot(&self) -> ShellResult {
        let sandboxed = !matches!(self.sandbox_type, SandboxType::None);
        let (stdout_full, stderr_full, _, _) = self.full_output();
        let (stdout, stdout_meta) = truncate_with_meta(&stdout_full);
        let (stderr, stderr_meta) = truncate_with_meta(&stderr_full);
        ShellResult {
            task_id: Some(self.id.clone()),
            status: self.status.clone(),
            exit_code: self.exit_code,
            stdout,
            stderr,
            duration_ms: u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
            stdout_len: stdout_meta.original_len,
            stderr_len: stderr_meta.original_len,
            stdout_omitted: stdout_meta.omitted,
            stderr_omitted: stderr_meta.omitted,
            stdout_truncated: stdout_meta.truncated,
            stderr_truncated: stderr_meta.truncated,
            sandboxed,
            sandbox_type: if sandboxed {
                Some(self.sandbox_type.to_string())
            } else {
                None
            },
            sandbox_denied: self.sandbox_denied(),
        }
    }

    pub(in crate::tools::shell) fn job_snapshot(&self) -> ShellJobSnapshot {
        let (stdout_full, stderr_full, stdout_len, stderr_len) = self.full_output();
        ShellJobSnapshot {
            id: self.id.clone(),
            job_id: self.id.clone(),
            command: self.command.clone(),
            cwd: self.working_dir.clone(),
            status: self.status.clone(),
            exit_code: self.exit_code,
            elapsed_ms: u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
            stdout_tail: tail_text(&stdout_full, 1200),
            stderr_tail: tail_text(&stderr_full, 1200),
            stdout_len,
            stderr_len,
            stdin_available: self.stdin.is_some() && self.status == ShellStatus::Running,
            stale: false,
            linked_task_id: self.linked_task_id.clone(),
        }
    }

    pub(in crate::tools::shell) fn job_detail(&self) -> ShellJobDetail {
        let (stdout, stderr, _, _) = self.full_output();
        ShellJobDetail {
            snapshot: self.job_snapshot(),
            stdout,
            stderr,
        }
    }
}

impl Drop for BackgroundShell {
    fn drop(&mut self) {
        if self.status == ShellStatus::Running
            && let Some(ref mut child) = self.child
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(in crate::tools::shell) fn take_delta_from_buffer(
    buffer: &Arc<Mutex<Vec<u8>>>,
    cursor: &mut usize,
) -> (Vec<u8>, usize) {
    let data = buffer.lock().map(|d| d.clone()).unwrap_or_default();
    let start = (*cursor).min(data.len());
    let delta = data[start..].to_vec();
    *cursor = data.len();
    (delta, data.len())
}

pub(in crate::tools::shell) fn tail_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let tail = text
        .chars()
        .rev()
        .take(max_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("...{tail}")
}
