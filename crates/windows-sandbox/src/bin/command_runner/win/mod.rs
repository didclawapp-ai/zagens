//! Elevated-path command runner: IPC pipes + restricted-token child spawn.

#![allow(unsafe_op_in_unsafe_fn)]

use std::fs::File;
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_GENERIC_READ, FILE_GENERIC_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::TerminateProcess;

use zagens_windows_sandbox::{
    ErrorPayload, ExitPayload, FramedMessage, IPC_PROTOCOL_VERSION, Message, OutputPayload,
    OutputStream, SpawnReady, SpawnRequest, SpawnStdio, create_restricted_token_with_capabilities,
    decode_bytes, encode_bytes, extract_spawn_denial_code, read_frame, read_handle_loop,
    spawn_with_stdio, to_wide, write_frame,
};

struct OwnedWinHandle(HANDLE);

impl OwnedWinHandle {
    fn new(handle: HANDLE) -> Self {
        Self(handle)
    }

    fn into_raw(mut self) -> HANDLE {
        let handle = self.0;
        self.0 = 0;
        handle
    }
}

impl Drop for OwnedWinHandle {
    fn drop(&mut self) {
        if self.0 != 0 && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

fn open_pipe(name: &str, access: u32) -> Result<HANDLE> {
    let path = to_wide(name);
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            0,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(anyhow::anyhow!(
            "CreateFileW failed for pipe {name}: {}",
            unsafe { windows_sys::Win32::Foundation::GetLastError() }
        ));
    }
    Ok(handle)
}

fn send_error(
    writer: &Arc<Mutex<File>>,
    code: &str,
    message: String,
    win32_code: Option<u32>,
) -> Result<()> {
    let msg = FramedMessage {
        version: IPC_PROTOCOL_VERSION,
        message: Message::Error {
            payload: ErrorPayload {
                message,
                code: code.to_string(),
                win32_code,
            },
        },
    };
    if let Ok(mut guard) = writer.lock() {
        write_frame(&mut *guard, &msg)?;
    }
    Ok(())
}

fn read_spawn_request(reader: &mut File) -> Result<SpawnRequest> {
    let Some(msg) = read_frame(reader)? else {
        anyhow::bail!("runner: pipe closed before spawn_request");
    };
    if msg.version != IPC_PROTOCOL_VERSION {
        anyhow::bail!("runner: unsupported protocol version {}", msg.version);
    }
    match msg.message {
        Message::SpawnRequest { payload } => Ok(*payload),
        other => anyhow::bail!("runner: expected spawn_request, got {other:?}"),
    }
}

unsafe fn create_job_kill_on_close() -> Result<HANDLE> {
    let h_job = OwnedWinHandle::new(CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()));
    if h_job.0 == 0 {
        anyhow::bail!("CreateJobObjectW failed");
    }
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let ok = SetInformationJobObject(
        h_job.0,
        JobObjectExtendedLimitInformation,
        &mut limits as *mut _ as *mut _,
        std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
    );
    if ok == 0 {
        anyhow::bail!("SetInformationJobObject failed");
    }
    Ok(h_job.into_raw())
}

pub fn main() -> Result<()> {
    let mut pipe_in = None;
    let mut pipe_out = None;
    for arg in std::env::args().skip(1) {
        if let Some(rest) = arg.strip_prefix("--pipe-in=") {
            pipe_in = Some(rest.to_string());
        } else if let Some(rest) = arg.strip_prefix("--pipe-out=") {
            pipe_out = Some(rest.to_string());
        }
    }
    let Some(pipe_in) = pipe_in else {
        anyhow::bail!("runner: no pipe-in provided");
    };
    let Some(pipe_out) = pipe_out else {
        anyhow::bail!("runner: no pipe-out provided");
    };

    let h_pipe_in = OwnedWinHandle::new(open_pipe(&pipe_in, FILE_GENERIC_READ)?);
    let h_pipe_out = OwnedWinHandle::new(open_pipe(&pipe_out, FILE_GENERIC_WRITE)?);
    let mut pipe_read = unsafe { File::from_raw_handle(h_pipe_in.into_raw() as RawHandle) };
    let pipe_write = Arc::new(Mutex::new(unsafe {
        File::from_raw_handle(h_pipe_out.into_raw() as RawHandle)
    }));

    let req = match read_spawn_request(&mut pipe_read) {
        Ok(v) => v,
        Err(err) => {
            let _ = send_error(&pipe_write, "spawn_failed", err.to_string(), None);
            return Err(err);
        }
    };

    if req.tty {
        let err = anyhow::anyhow!("runner: tty mode is not supported in this build");
        let _ = send_error(&pipe_write, "spawn_failed", err.to_string(), None);
        return Err(err);
    }

    let cap_refs: Vec<&str> = req.cap_sids.iter().map(String::as_str).collect();
    if cap_refs.is_empty() {
        let err = anyhow::anyhow!("runner: empty capability SID list");
        let _ = send_error(&pipe_write, "spawn_failed", err.to_string(), None);
        return Err(err);
    }

    let token = match create_restricted_token_with_capabilities(&cap_refs)
        .context("create restricted token")
    {
        Ok(v) => v,
        Err(err) => {
            let _ = send_error(
                &pipe_write,
                "spawn_failed",
                err.to_string(),
                extract_spawn_denial_code(&err),
            );
            return Err(err);
        }
    };

    let child = match spawn_with_stdio(
        token.handle(),
        &req.command,
        &req.cwd,
        &req.env,
        SpawnStdio {
            capture_stdout: true,
            capture_stderr: true,
            stdin_open: req.stdin_open,
            stdin_data: None,
        },
    ) {
        Ok(v) => v,
        Err(err) => {
            // PR-2.13: forward the structured Win32 code (e.g. 5 / 1385) so
            // the parent reports `sandbox_denial_code` instead of guessing.
            let _ = send_error(
                &pipe_write,
                "spawn_failed",
                err.to_string(),
                extract_spawn_denial_code(&err),
            );
            return Err(err);
        }
    };

    let child = Arc::new(Mutex::new(child));
    if let Some(job) = unsafe { create_job_kill_on_close().ok() } {
        let process_handle = child
            .lock()
            .map_err(|_| anyhow::anyhow!("child lock poisoned"))?
            .process_handle();
        unsafe {
            let _ = AssignProcessToJobObject(job, process_handle);
            CloseHandle(job);
        }
    }

    let process_id = child
        .lock()
        .map_err(|_| anyhow::anyhow!("child lock poisoned"))?
        .process_id();
    let ready = FramedMessage {
        version: IPC_PROTOCOL_VERSION,
        message: Message::SpawnReady {
            payload: SpawnReady { process_id },
        },
    };
    if let Ok(mut guard) = pipe_write.lock() {
        write_frame(&mut *guard, &ready)?;
    }

    let (stdout_h, stderr_h) = child
        .lock()
        .map_err(|_| anyhow::anyhow!("child lock poisoned"))?
        .detach_output_readers();
    let writer_out = Arc::clone(&pipe_write);
    let out_thread = read_handle_loop(stdout_h, move |chunk| {
        let msg = FramedMessage {
            version: IPC_PROTOCOL_VERSION,
            message: Message::Output {
                payload: OutputPayload {
                    data_b64: encode_bytes(chunk),
                    stream: OutputStream::Stdout,
                },
            },
        };
        if let Ok(mut guard) = writer_out.lock() {
            let _ = write_frame(&mut *guard, &msg);
        }
    });
    let writer_err = Arc::clone(&pipe_write);
    let err_thread = read_handle_loop(stderr_h, move |chunk| {
        let msg = FramedMessage {
            version: IPC_PROTOCOL_VERSION,
            message: Message::Output {
                payload: OutputPayload {
                    data_b64: encode_bytes(chunk),
                    stream: OutputStream::Stderr,
                },
            },
        };
        if let Ok(mut guard) = writer_err.lock() {
            let _ = write_frame(&mut *guard, &msg);
        }
    });

    let child_for_input = Arc::clone(&child);
    let input_thread = std::thread::spawn(move || {
        loop {
            let msg = match read_frame(&mut pipe_read) {
                Ok(Some(v)) => v,
                Ok(None) | Err(_) => break,
            };
            match msg.message {
                Message::Stdin { payload } => {
                    if let Ok(bytes) = decode_bytes(&payload.data_b64) {
                        if let Ok(mut guard) = child_for_input.lock() {
                            let _ = guard.write_stdin(&bytes);
                        }
                    }
                }
                Message::CloseStdin { .. } => {
                    if let Ok(mut guard) = child_for_input.lock() {
                        guard.close_stdin();
                    }
                }
                Message::Terminate { .. } => {
                    if let Ok(mut guard) = child_for_input.lock() {
                        let _ = guard.kill();
                    }
                    break;
                }
                _ => {}
            }
        }
    });

    let timeout = req.timeout_ms.map(Duration::from_millis);
    let deadline = timeout.map(|t| Instant::now() + t);
    let mut timed_out = false;
    let process_handle = child
        .lock()
        .map_err(|_| anyhow::anyhow!("child lock poisoned"))?
        .process_handle();

    loop {
        let exit_code = {
            let guard = child
                .lock()
                .map_err(|_| anyhow::anyhow!("child lock poisoned"))?;
            match guard.try_wait()? {
                Some(code) => code,
                None => {
                    drop(guard);
                    if let Some(deadline) = deadline {
                        if Instant::now() >= deadline {
                            timed_out = true;
                            unsafe {
                                let _ = TerminateProcess(process_handle, 1);
                            }
                            std::thread::sleep(Duration::from_millis(50));
                            continue;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
            }
        };

        let exit_msg = FramedMessage {
            version: IPC_PROTOCOL_VERSION,
            message: Message::Exit {
                payload: ExitPayload {
                    exit_code: i32::try_from(exit_code).unwrap_or(-1),
                    timed_out,
                },
            },
        };
        if let Ok(mut guard) = pipe_write.lock() {
            write_frame(&mut *guard, &exit_msg)?;
        }
        break;
    }

    let _ = input_thread.join();
    let _ = out_thread.join();
    let _ = err_thread.join();
    Ok(())
}
