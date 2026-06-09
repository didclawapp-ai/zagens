//! Synchronous elevated capture via sandbox-user logon + command-runner IPC.

use std::time::Duration;

use anyhow::{Result, anyhow};

use crate::elevated::ipc::{
    FramedMessage, IPC_PROTOCOL_VERSION, Message, OutputStream, StdinPayload, decode_bytes,
    read_frame, write_frame,
};
use crate::elevated::runner_client::runner_error_to_anyhow;
use crate::elevated::session::start_runner_session;
use crate::plan::WindowsExecPlan;
use crate::process::CapturedOutput;

pub fn spawn_sync(
    plan: &WindowsExecPlan,
    stdin_data: Option<&str>,
    timeout: Option<Duration>,
) -> Result<CapturedOutput> {
    let (mut pipe_write, mut pipe_read) =
        start_runner_session(plan, stdin_data.is_some(), timeout)?;

    if let Some(stdin) = stdin_data {
        let stdin_msg = FramedMessage {
            version: IPC_PROTOCOL_VERSION,
            message: Message::Stdin {
                payload: StdinPayload {
                    data_b64: crate::elevated::ipc::encode_bytes(stdin.as_bytes()),
                },
            },
        };
        write_frame(&mut pipe_write, &stdin_msg)?;
        let close_msg = FramedMessage {
            version: IPC_PROTOCOL_VERSION,
            message: Message::CloseStdin {
                payload: crate::elevated::ipc::EmptyPayload::default(),
            },
        };
        write_frame(&mut pipe_write, &close_msg)?;
    }

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let (exit_code, _timed_out) = loop {
        let msg = match read_frame(&mut pipe_read) {
            Ok(Some(msg)) => msg,
            Ok(None) => break Err(anyhow!("runner pipe closed before exit")),
            Err(err) => break Err(err),
        };
        match msg.message {
            Message::SpawnReady { .. } => {}
            Message::Output { payload } => {
                let bytes = decode_bytes(&payload.data_b64)?;
                match payload.stream {
                    OutputStream::Stdout => stdout.extend_from_slice(&bytes),
                    OutputStream::Stderr => stderr.extend_from_slice(&bytes),
                }
            }
            Message::Exit { payload } => {
                break Ok((payload.exit_code, payload.timed_out));
            }
            Message::Error { payload } => {
                break Err(runner_error_to_anyhow(&payload));
            }
            other => {
                break Err(anyhow!(
                    "unexpected runner message during capture: {other:?}"
                ));
            }
        }
    }?;

    drop(pipe_write);

    Ok(CapturedOutput {
        exit_code: u32::try_from(exit_code).unwrap_or(u32::MAX),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}
