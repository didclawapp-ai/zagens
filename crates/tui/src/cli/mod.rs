//! CLI surface for the `deepseek-tui` binary (B3).
//!
//! Command implementations remain in `main.rs` for now; this module holds
//! `clap` definitions so `main.rs` can shrink incrementally.

pub mod args;
pub mod commands;

pub use args::*;
pub use commands::*;
