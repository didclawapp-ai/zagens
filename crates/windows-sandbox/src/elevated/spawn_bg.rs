//! Elevated background spawn: a runner IPC session exposed as a managed child
//! (`exec_shell` background jobs / `write_stdin` under the elevated sandbox).
//!
//! The runner process streams the child's stdout/stderr as IPC frames, so the
//! parent has no child process HANDLE. Instead, a pump thread decodes frames
//! into caller-provided sinks and records the exit state for `try_wait`.

use std::fs::File;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

use crate::elevated::ipc::{
    EmptyPayload, FramedMessage, IPC_PROTOCOL_VERSION, Message, OutputStream, StdinPayload,
    decode_bytes, encode_bytes, read_frame, write_frame,
};
use crate::elevated::session::start_runner_session;
use crate::plan::WindowsExecPlan;

#[derive(Default)]
struct ExitState {
    exit_code: Option<u32>,
    timed_out: bool,
    /// Structured Win32 denial code reported by the runner (PR-2.13).
    denial_code: Option<u32>,
}

/// Background child running under the elevated sandbox (runner IPC session).
pub struct ElevatedChild {
    pipe_write: Option<Arc<Mutex<File>>>,
    /// Taken by [`ElevatedChild::start_output_pump`].
    pipe_read: Option<File>,
    state: Arc<Mutex<ExitState>>,
}

impl ElevatedChild {
    /// Starts the runner session and sends optional initial stdin data.
    /// Call [`ElevatedChild::start_output_pump`] next to begin streaming.
    pub fn spawn(plan: &WindowsExecPlan, stdin_data: Option<&str>) -> Result<Self> {
        let (pipe_write, pipe_read) = start_runner_session(plan, true, None)?;
        let mut child = Self {
            pipe_write: Some(Arc::new(Mutex::new(pipe_write))),
            pipe_read: Some(pipe_read),
            state: Arc::new(Mutex::new(ExitState::default())),
        };
        if let Some(data) = stdin_data {
            child.write_stdin(data.as_bytes())?;
        }
        Ok(child)
    }

    /// Spawns the frame-pump thread. Stdout/stderr bytes are forwarded to the
    /// sinks; the thread exits on the runner's exit/error frame or pipe EOF.
    pub fn start_output_pump<F, G>(
        &mut self,
        mut on_stdout: F,
        mut on_stderr: G,
    ) -> Result<JoinHandle<()>>
    where
        F: FnMut(&[u8]) + Send + 'static,
        G: FnMut(&[u8]) + Send + 'static,
    {
        let mut pipe_read = self
            .pipe_read
            .take()
            .ok_or_else(|| anyhow!("output pump already started"))?;
        let state = Arc::clone(&self.state);
        let handle = std::thread::Builder::new()
            .name("zagens-elevated-pump".to_string())
            .spawn(move || {
                let set_exit = |code: u32, timed_out: bool, denial: Option<u32>| {
                    if let Ok(mut guard) = state.lock() {
                        if guard.exit_code.is_none() {
                            guard.exit_code = Some(code);
                            guard.timed_out = timed_out;
                            guard.denial_code = denial;
                        }
                    }
                };
                loop {
                    let msg = match read_frame(&mut pipe_read) {
                        Ok(Some(msg)) => msg,
                        // EOF / broken pipe without an exit frame: the runner
                        // died — surface a generic failure code.
                        Ok(None) | Err(_) => {
                            set_exit(1, false, None);
                            break;
                        }
                    };
                    match msg.message {
                        Message::Output { payload } => {
                            if let Ok(bytes) = decode_bytes(&payload.data_b64) {
                                match payload.stream {
                                    OutputStream::Stdout => on_stdout(&bytes),
                                    OutputStream::Stderr => on_stderr(&bytes),
                                }
                            }
                        }
                        Message::Exit { payload } => {
                            set_exit(
                                u32::try_from(payload.exit_code).unwrap_or(u32::MAX),
                                payload.timed_out,
                                None,
                            );
                            break;
                        }
                        Message::Error { payload } => {
                            on_stderr(payload.message.as_bytes());
                            on_stderr(b"\n");
                            set_exit(payload.win32_code.unwrap_or(1), false, payload.win32_code);
                            break;
                        }
                        _ => {}
                    }
                }
            })?;
        Ok(handle)
    }

    /// Non-blocking exit check (`None` while the child is still running).
    pub fn try_wait(&self) -> Result<Option<u32>> {
        Ok(self
            .state
            .lock()
            .map_err(|_| anyhow!("elevated child state poisoned"))?
            .exit_code)
    }

    /// Blocks until the child exits (polling the pump-recorded state).
    pub fn wait(&mut self, timeout: Option<Duration>) -> Result<u32> {
        let deadline = timeout.map(|t| Instant::now() + t);
        loop {
            if let Some(code) = self.try_wait()? {
                return Ok(code);
            }
            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    let _ = self.kill();
                }
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Structured Win32 denial code from the runner, if the spawn was denied.
    pub fn denial_code(&self) -> Option<u32> {
        self.state.lock().ok().and_then(|guard| guard.denial_code)
    }

    pub fn write_stdin(&mut self, data: &[u8]) -> Result<()> {
        self.send_frame(Message::Stdin {
            payload: StdinPayload {
                data_b64: encode_bytes(data),
            },
        })
    }

    pub fn close_stdin(&mut self) {
        let _ = self.send_frame(Message::CloseStdin {
            payload: EmptyPayload::default(),
        });
    }

    /// Asks the runner to terminate the child tree. The runner then reports
    /// the final exit frame, which the pump records as usual.
    pub fn kill(&mut self) -> Result<()> {
        let _ = self.send_frame(Message::Terminate {
            payload: EmptyPayload::default(),
        });
        Ok(())
    }

    fn send_frame(&mut self, message: Message) -> Result<()> {
        let Some(pipe) = self.pipe_write.as_ref() else {
            return Err(anyhow!("runner pipe is closed"));
        };
        let frame = FramedMessage {
            version: IPC_PROTOCOL_VERSION,
            message,
        };
        let mut guard = pipe
            .lock()
            .map_err(|_| anyhow!("runner pipe lock poisoned"))?;
        write_frame(&mut *guard, &frame)
    }
}
