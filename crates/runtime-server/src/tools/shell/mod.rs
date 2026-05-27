//! Advanced shell execution with background process support and sandboxing.
//!
//! Provides:
//! - Synchronous command execution with timeout
//! - Background process execution
//! - Process output retrieval
//! - Process termination
//! - Sandbox support (macOS Seatbelt)
//! - Streaming output (future)

mod host;
mod manager;
mod process;
mod tools;
mod types;

pub use host::{SharedShellManager, TuiShellHost, new_shared_shell_manager};
pub use manager::ShellManager;
pub use process::BackgroundShell;
pub use tools::{
    ExecShellTool, NoteTool, ShellCancelTool, ShellInteractTool, ShellWaitTool,
};
pub use types::{
    ShellDeltaResult, ShellJobDetail, ShellJobSnapshot, ShellResult, ShellStatus,
};

#[cfg(test)]
mod tests;
