//! [`ShellHost`] — engine boundary marker for the shell tool subsystem.
//!
//! **Intentionally empty in M3.** The live `Engine`
//! (`crates/tui/src/core/engine/*`) never invokes any method on the
//! `shell_manager` field — it only clones the `SharedShellManager` into
//! `ToolContext` via `build_tool_context`. The single call site is
//! `crates/tui/src/core/engine/tool_context.rs:30`.
//!
//! Introducing the trait now (per spike §2.2 target layout) lets M7
//! swap `shell_manager: SharedShellManager` to `Box<dyn ShellHost>`
//! without simultaneously inventing the trait surface. M5/M7 may later
//! extend with command acquisition / progress registration once the
//! `RuntimeToolServices` consolidation lands.

/// Engine-side shell host marker.
///
/// Implemented by `crates/tui/src/tools/shell.rs`'s `ShellManager` (an
/// empty `impl ShellHost for ShellManager` block). Existing call sites
/// that need to actually invoke shell methods continue to go through
/// the concrete `SharedShellManager` (e.g. via `ToolContext`).
pub trait ShellHost: Send + Sync {}
