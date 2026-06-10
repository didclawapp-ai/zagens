//! Spawn helper for sandboxed child processes (restricted token + Job Object).

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;
use std::time::Duration;

use anyhow::{Result, anyhow};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
    SetHandleInformation,
};
use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows_sys::Win32::System::Console::{GetStdHandle, HPCON, STD_ERROR_HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, GetExitCodeProcess, GetProcessId, INFINITE,
    PROCESS_INFORMATION, TerminateProcess, WaitForSingleObject,
};

use crate::process_startup::StartupWithHandleList;
use crate::winutil::to_wide;

pub struct CapturedOutput {
    pub exit_code: u32,
    pub stdout: String,
    pub stderr: String,
}

/// Structured spawn-denial error (PR-2.13): carries the raw Win32 error code
/// from a failed `CreateProcessAsUserW` / logon call so callers can surface
/// `sandbox_denial_code` instead of parsing stderr heuristically.
#[derive(Debug)]
pub struct SpawnDenial {
    pub win32_code: u32,
    pub api: &'static str,
}

impl std::fmt::Display for SpawnDenial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} failed with Win32 error {}: {}",
            self.api,
            self.win32_code,
            crate::winutil::format_last_error(self.win32_code as i32)
        )
    }
}

impl std::error::Error for SpawnDenial {}

/// Extract the Win32 denial code from an error chain, if present.
pub fn extract_spawn_denial_code(err: &anyhow::Error) -> Option<u32> {
    err.chain()
        .find_map(|cause| cause.downcast_ref::<SpawnDenial>())
        .map(|denial| denial.win32_code)
}

#[derive(Debug, Clone, Default)]
pub struct SpawnOptions {
    pub private_desktop: bool,
    pub tty: bool,
    pub conpty_rows: u16,
    pub conpty_cols: u16,
}

#[derive(Debug, Clone, Default)]
pub struct SpawnStdio {
    pub capture_stdout: bool,
    pub capture_stderr: bool,
    /// Keep stdin pipe open for later writes (background `write_stdin`).
    pub stdin_open: bool,
    pub stdin_data: Option<String>,
}

pub struct ManagedProcess {
    process: HANDLE,
    job: HANDLE,
    stdout_read: HANDLE,
    stderr_read: HANDLE,
    stdin_write: HANDLE,
    conpty: HPCON,
}

impl ManagedProcess {
    pub(crate) fn from_spawn_handles(
        process: HANDLE,
        job: HANDLE,
        stdout_read: HANDLE,
        stderr_read: HANDLE,
        stdin_write: HANDLE,
        conpty: HPCON,
    ) -> Self {
        Self {
            process,
            job,
            stdout_read,
            stderr_read,
            stdin_write,
            conpty,
        }
    }

    pub fn resize_conpty(&self, rows: u16, cols: u16) -> Result<()> {
        if self.conpty == 0 {
            anyhow::bail!("process is not attached to a ConPTY");
        }
        crate::conpty::resize_conpty(self.conpty, rows, cols)
    }

    /// Close the pseudo console after the child exits so ConPTY output pipes EOF
    /// and the runner can drain IPC before sending `Exit`.
    pub fn finalize_conpty_after_exit(&mut self) {
        if self.conpty != 0 {
            unsafe {
                windows_sys::Win32::System::Console::ClosePseudoConsole(self.conpty);
            }
            self.conpty = 0;
        }
    }

    pub fn detach_output_readers(&mut self) -> (HANDLE, HANDLE) {
        let readers = (self.stdout_read, self.stderr_read);
        self.stdout_read = 0;
        self.stderr_read = 0;
        readers
    }

    pub fn try_wait(&self) -> Result<Option<u32>> {
        unsafe {
            let wait = WaitForSingleObject(self.process, 0);
            if wait == 0x0000_0102 {
                return Ok(None);
            }
            let mut code: u32 = 0;
            GetExitCodeProcess(self.process, &mut code);
            if code == 259 {
                Ok(None)
            } else {
                Ok(Some(code))
            }
        }
    }

    pub fn process_handle(&self) -> HANDLE {
        self.process
    }

    pub fn process_id(&self) -> u32 {
        if self.process == 0 {
            return 0;
        }
        unsafe { GetProcessId(self.process) }
    }

    pub fn write_stdin(&mut self, data: &[u8]) -> Result<()> {
        if self.stdin_write == 0 {
            return Err(anyhow!("stdin pipe is not open"));
        }
        unsafe { write_pipe(self.stdin_write, data) }
    }

    pub fn close_stdin(&mut self) {
        if self.stdin_write != 0 {
            unsafe {
                CloseHandle(self.stdin_write);
            }
            self.stdin_write = 0;
        }
    }

    pub fn stdout_read_handle(&self) -> HANDLE {
        self.stdout_read
    }

    pub fn stderr_read_handle(&self) -> HANDLE {
        self.stderr_read
    }

    pub fn wait(&mut self, timeout: Option<Duration>) -> Result<CapturedOutput> {
        let wait_ms = timeout.map(|d| d.as_millis().min(u32::MAX as u128) as u32);
        unsafe {
            let wait = match wait_ms {
                Some(ms) => WaitForSingleObject(self.process, ms),
                None => WaitForSingleObject(self.process, INFINITE),
            };
            if wait_ms.is_some() && wait == 0x0000_0102 {
                TerminateProcess(self.process, 1);
            }
            let mut code: u32 = 0;
            GetExitCodeProcess(self.process, &mut code);
            let stdout = read_pipe(self.stdout_read)?;
            let stderr = read_pipe(self.stderr_read)?;
            CloseHandle(self.process);
            CloseHandle(self.job);
            CloseHandle(self.stdout_read);
            CloseHandle(self.stderr_read);
            if self.stdin_write != 0 {
                CloseHandle(self.stdin_write);
            }
            self.process = 0;
            self.job = 0;
            self.stdout_read = 0;
            self.stderr_read = 0;
            self.stdin_write = 0;
            Ok(CapturedOutput {
                exit_code: code,
                stdout,
                stderr,
            })
        }
    }

    pub fn kill(&mut self) -> Result<()> {
        let pid = self.process_id();
        if pid != 0 {
            kill_process_tree_best_effort(pid);
        }
        unsafe {
            if self.process != 0 {
                let _ = TerminateProcess(self.process, 1);
            }
        }
        Ok(())
    }
}

impl Drop for ManagedProcess {
    fn drop(&mut self) {
        unsafe {
            if self.conpty != 0 {
                windows_sys::Win32::System::Console::ClosePseudoConsole(self.conpty);
            }
            for h in [
                self.process,
                self.job,
                self.stdout_read,
                self.stderr_read,
                self.stdin_write,
            ] {
                if h != 0 {
                    CloseHandle(h);
                }
            }
        }
    }
}

pub fn spawn_with_stdio(
    token: HANDLE,
    argv: &[String],
    cwd: &Path,
    env: &HashMap<String, String>,
    stdio: SpawnStdio,
    opts: SpawnOptions,
) -> Result<ManagedProcess> {
    if opts.tty {
        let (process, hpc) = crate::conpty::spawn_with_conpty(
            token,
            argv,
            cwd,
            env,
            if opts.conpty_rows == 0 {
                crate::conpty::DEFAULT_CONPTY_ROWS
            } else {
                opts.conpty_rows
            },
            if opts.conpty_cols == 0 {
                crate::conpty::DEFAULT_CONPTY_COLS
            } else {
                opts.conpty_cols
            },
            opts.private_desktop,
        )?;
        let mut process = process;
        // `spawn_with_conpty` returns hpc for resize; attach before ConPtyAttributeList drops.
        process.conpty = hpc;
        return Ok(process);
    }

    unsafe {
        let mut stdout_r: HANDLE = 0;
        let mut stdout_w: HANDLE = 0;
        let mut stderr_r: HANDLE = 0;
        let mut stderr_w: HANDLE = 0;
        let mut stdin_r: HANDLE = 0;
        let mut stdin_w: HANDLE = 0;

        if stdio.capture_stdout
            && CreatePipe(&mut stdout_r, &mut stdout_w, std::ptr::null_mut(), 0) == 0
        {
            return Err(anyhow!("CreatePipe(stdout) failed: {}", GetLastError()));
        }
        if stdio.capture_stderr
            && CreatePipe(&mut stderr_r, &mut stderr_w, std::ptr::null_mut(), 0) == 0
        {
            return Err(anyhow!("CreatePipe(stderr) failed: {}", GetLastError()));
        }
        let needs_stdin_pipe = stdio.stdin_open || stdio.stdin_data.is_some();
        if needs_stdin_pipe && CreatePipe(&mut stdin_r, &mut stdin_w, std::ptr::null_mut(), 0) == 0
        {
            return Err(anyhow!("CreatePipe(stdin) failed: {}", GetLastError()));
        }
        // Restricted-token spawns require real inheritable stdio handles in the
        // PROC_THREAD_ATTRIBUTE_HANDLE_LIST. A console-less runner's
        // GetStdHandle(STD_INPUT_HANDLE) is invalid and breaks child stdio.
        if !needs_stdin_pipe
            && (stdio.capture_stdout || stdio.capture_stderr)
            && CreatePipe(&mut stdin_r, &mut stdin_w, std::ptr::null_mut(), 0) == 0
        {
            return Err(anyhow!(
                "CreatePipe(stdin fallback) failed: {}",
                GetLastError()
            ));
        }

        for h in [stdout_w, stderr_w, stdin_r] {
            if h != 0 {
                SetHandleInformation(h, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
            }
        }

        let cmdline_str = argv_to_command_line(argv);
        let mut cmdline = to_wide(&cmdline_str);
        let env_block = if env.is_empty() {
            None
        } else {
            Some(make_env_block(env))
        };
        let cwd_wide = to_wide(cwd);

        let h_stdin = stdin_r;
        let h_stdout = if stdout_w != 0 {
            stdout_w
        } else {
            GetStdHandle(windows_sys::Win32::System::Console::STD_OUTPUT_HANDLE)
        };
        let h_stderr = if stderr_w != 0 {
            stderr_w
        } else {
            GetStdHandle(STD_ERROR_HANDLE)
        };
        let desktop = crate::private_desktop::desktop_wide_name(opts.private_desktop)?;
        let startup =
            StartupWithHandleList::new(h_stdin, h_stdout, h_stderr, desktop.as_ptr() as *mut u16)?;

        let env_ptr = env_block
            .as_ref()
            .map_or(std::ptr::null_mut(), |block| block.as_ptr() as *mut c_void);

        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
        let ok = CreateProcessAsUserW(
            token,
            std::ptr::null(),
            cmdline.as_mut_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            1,
            startup.creation_flags(CREATE_UNICODE_ENVIRONMENT),
            env_ptr,
            cwd_wide.as_ptr(),
            startup.startup_info_ptr(),
            &mut pi,
        );

        if stdout_w != 0 {
            CloseHandle(stdout_w);
        }
        if stderr_w != 0 {
            CloseHandle(stderr_w);
        }
        if stdin_r != 0 {
            CloseHandle(stdin_r);
        }
        if !needs_stdin_pipe && stdin_w != 0 {
            CloseHandle(stdin_w);
            stdin_w = 0;
        }

        if ok == 0 {
            let err = GetLastError();
            for h in [stdout_r, stderr_r, stdin_w] {
                if h != 0 {
                    CloseHandle(h);
                }
            }
            return Err(anyhow::Error::new(SpawnDenial {
                win32_code: err,
                api: "CreateProcessAsUserW",
            }));
        }

        CloseHandle(pi.hThread);

        if let Some(input) = &stdio.stdin_data {
            if stdin_w != 0 {
                write_pipe(stdin_w, input.as_bytes())?;
                if !stdio.stdin_open {
                    CloseHandle(stdin_w);
                    stdin_w = 0;
                }
            }
        }

        let job = create_kill_on_close_job()?;
        if AssignProcessToJobObject(job, pi.hProcess) == 0 {
            CloseHandle(job);
            CloseHandle(pi.hProcess);
            return Err(anyhow!(
                "AssignProcessToJobObject failed: {}",
                GetLastError()
            ));
        }

        Ok(ManagedProcess {
            process: pi.hProcess,
            job,
            stdout_read: stdout_r,
            stderr_read: stderr_r,
            stdin_write: stdin_w,
            conpty: 0,
        })
    }
}

pub fn run_as_user(
    token: HANDLE,
    argv: &[String],
    cwd: &Path,
    env: &HashMap<String, String>,
) -> Result<CapturedOutput> {
    let process = spawn_with_stdio(
        token,
        argv,
        cwd,
        env,
        SpawnStdio {
            capture_stdout: true,
            capture_stderr: true,
            stdin_open: false,
            stdin_data: None,
        },
        SpawnOptions::default(),
    )?;
    let mut process = process;
    process.wait(None)
}

unsafe fn create_kill_on_close_job() -> Result<HANDLE> {
    let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
    if job == 0 {
        return Err(anyhow!("CreateJobObjectW failed: {}", GetLastError()));
    }
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if SetInformationJobObject(
        job,
        JobObjectExtendedLimitInformation,
        &mut info as *mut _ as *mut c_void,
        std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
    ) == 0
    {
        CloseHandle(job);
        return Err(anyhow!(
            "SetInformationJobObject failed: {}",
            GetLastError()
        ));
    }
    Ok(job)
}

pub(crate) fn make_env_block(env: &HashMap<String, String>) -> Vec<u16> {
    let mut items: Vec<(String, String)> =
        env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    items.sort_by(|a, b| {
        a.0.to_uppercase()
            .cmp(&b.0.to_uppercase())
            .then(a.0.cmp(&b.0))
    });
    let mut w: Vec<u16> = Vec::new();
    for (k, v) in items {
        let mut s = to_wide(format!("{k}={v}"));
        s.pop();
        w.extend_from_slice(&s);
        w.push(0);
    }
    w.push(0);
    w
}

fn kill_process_tree_best_effort(pid: u32) {
    use std::process::Stdio;
    let _ = std::process::Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn quote_arg(arg: &str) -> String {
    if arg.is_empty() || arg.chars().any(|c| c.is_whitespace() || c == '"') {
        let mut q = String::from("\"");
        for ch in arg.chars() {
            if ch == '"' {
                q.push_str("\\\"");
            } else {
                q.push(ch);
            }
        }
        q.push('"');
        q
    } else {
        arg.to_string()
    }
}

pub(crate) fn argv_to_command_line(argv: &[String]) -> String {
    // `cmd /C <tail>`: `<tail>` is already a full user command from `plan.rs`
    // (`type "C:\…"`, redirects, etc.). Re-quoting the tail breaks parsing.
    if argv.len() == 3 && argv[0].eq_ignore_ascii_case("cmd") && argv[1].eq_ignore_ascii_case("/C")
    {
        return format!("{} {} {}", quote_arg(&argv[0]), argv[1], argv[2]);
    }
    argv.iter()
        .map(|a| quote_arg(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads a HANDLE until EOF and invokes `on_chunk` for each read.
pub fn read_handle_loop<F>(handle: HANDLE, mut on_chunk: F) -> std::thread::JoinHandle<()>
where
    F: FnMut(&[u8]) + Send + 'static,
{
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            let mut read_bytes: u32 = 0;
            let ok = unsafe {
                ReadFile(
                    handle,
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                    &mut read_bytes,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 || read_bytes == 0 {
                break;
            }
            on_chunk(&buf[..read_bytes as usize]);
        }
        unsafe {
            CloseHandle(handle);
        }
    })
}

unsafe fn read_pipe(h: HANDLE) -> Result<String> {
    if h == 0 || h == INVALID_HANDLE_VALUE {
        return Ok(String::new());
    }
    let mut buf = [0u8; 4096];
    let mut out = Vec::new();
    loop {
        let mut read: u32 = 0;
        if ReadFile(
            h,
            buf.as_mut_ptr(),
            buf.len() as u32,
            &mut read,
            std::ptr::null_mut(),
        ) == 0
        {
            break;
        }
        if read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read as usize]);
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

unsafe fn write_pipe(h: HANDLE, data: &[u8]) -> Result<()> {
    if h == 0 || data.is_empty() {
        return Ok(());
    }
    let mut offset = 0usize;
    while offset < data.len() {
        let mut written: u32 = 0;
        if WriteFile(
            h,
            data[offset..].as_ptr(),
            (data.len() - offset) as u32,
            &mut written,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(anyhow!("WriteFile failed: {}", GetLastError()));
        }
        if written == 0 {
            break;
        }
        offset += written as usize;
    }
    Ok(())
}
