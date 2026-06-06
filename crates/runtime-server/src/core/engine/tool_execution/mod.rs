//! Low-level tool execution helpers for the engine turn loop (R-003 A4.6).
//!
//! MCP dispatch, execution locking, and parallel fanout stay in tui L2;
//! see `deepseek-core::engine::turn_loop::tool_exec` for the core port trait.

mod exec;
mod mcp;
mod parallel;
mod port;
mod progress;
mod terminal_guard;

pub use port::{apply_tool_spillover_audit, detached_execute_with_lock, execute_plan_on_engine};

#[cfg(test)]
pub(crate) use port::McpPoolHandle;
