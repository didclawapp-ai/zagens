//! CLI surface for the `deepseek-tui` binary (B3).
//!
//! Command implementations remain in `main.rs` for now; this module holds
//! `clap` definitions so `main.rs` can shrink incrementally.

pub mod args;
#[cfg(feature = "tui-ui")]
pub mod commands;
pub mod entry;

pub use args::*;
#[cfg(feature = "tui-ui")]
pub use commands::*;
pub use entry::configure_windows_console_utf8;

#[cfg(test)]
mod tests;
