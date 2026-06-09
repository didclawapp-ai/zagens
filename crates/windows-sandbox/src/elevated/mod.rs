//! Elevated sandbox path: sandbox-user logon + command-runner IPC.

mod ipc;
mod runner_client;
mod runner_pipe;
mod session;
mod spawn_bg;
mod spawn_sync;

pub use ipc::{
    ErrorPayload, ExitPayload, FramedMessage, IPC_PROTOCOL_VERSION, Message, OutputPayload,
    OutputStream, SpawnReady, SpawnRequest, decode_bytes, encode_bytes, read_frame, write_frame,
};
pub use spawn_bg::ElevatedChild;
pub use spawn_sync::spawn_sync;
