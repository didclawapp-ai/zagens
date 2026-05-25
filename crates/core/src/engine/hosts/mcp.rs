//! [`McpHost`] — engine boundary for the MCP (Model Context Protocol)
//! tool pool.
//!
//! M4 (Engine-struct strangler step) promotes the empty
//! [`TurnLoopMcpPool`](crate::engine::turn_loop::TurnLoopMcpPool) marker
//! into a named host trait that exposes the MCP **predicate / metadata
//! surface** the live `Engine` and the core turn loop actually use:
//! `is_mcp_tool`, `tool_is_parallel_safe`, `tool_is_read_only`,
//! `tool_approval_description`. All four methods carry **default
//! implementations** that delegate to the existing stateless free
//! functions in [`crate::engine::dispatch`], so `impl McpHost for
//! McpPool {}` in tui is a one-liner that produces zero behavior
//! change.
//!
//! ## Why not include `execute_tool` / `ensure` / `shutdown` here?
//!
//! - **`execute_tool`** lives on
//!   [`McpPoolPort`](crate::engine::turn_loop::McpPoolPort) and is
//!   implemented on `McpPoolHandle = Arc<Mutex<McpPool>>` (the locked
//!   container, not the bare pool). The two traits have different
//!   `self` shapes; merging would require reworking the
//!   `mcp_pool_as_port` factory and rippling through every
//!   `Option<Arc<AsyncMutex<Self::McpPool>>>` parameter in the turn
//!   loop signature. M4 keeps them separate.
//! - **`ensure_pool` / `shutdown_all`** mutate engine state
//!   (`self.mcp_pool = Some(...)`) and depend on
//!   `EngineConfig.network_policy` + `session.mcp_config_path`. They
//!   stay as inherent `Engine` methods (`tool_context.rs:112-124`,
//!   `op_loop.rs:86-89`) and will move into the core `Engine` struct
//!   alongside the field in M7.
//!
//! ## Call-graph (R1)
//!
//! Method surface derived from direct calls to `McpPool::is_mcp_tool`
//! and the `mcp_tool_*` free functions across the Engine call tree:
//!
//! | Method                       | Call sites                                                                                                       |
//! |------------------------------|------------------------------------------------------------------------------------------------------------------|
//! | `is_mcp_tool`                | `tui::mcp.rs:1498` (inherent), `capacity_flow/interventions.rs:139,143`, `tool_execution/parallel.rs:15,33`, …   |
//! | `tool_is_parallel_safe`      | `capacity_flow/interventions.rs:144`, `tool_execution/parallel.rs:34`, `host_impl/mod.rs:427`                   |
//! | `tool_is_read_only`          | `capacity_flow/replay.rs:35`, `host_impl/mod.rs:426,428`                                                         |
//! | `tool_approval_description`  | `host_impl/mod.rs:429`                                                                                           |
//!
//! Existing call sites keep using the inherent
//! `McpPool::is_mcp_tool(name)` and the `mcp_tool_*` free fns
//! unchanged (M4 Q5A: zero call-site churn). The trait methods are
//! the parallel `&self` entry points future implementations (e.g.
//! MCP server capability negotiation) can override.

/// Engine-side MCP host.
///
/// Implemented by `crates/tui/src/core/engine/turn_loop/host_impl/mod.rs`
/// (`impl McpHost for McpPool {}`) using only the default method
/// bodies — the live `McpPool` type carries no extra state for
/// these predicates beyond what the free functions in
/// [`crate::engine::dispatch`] already encode.
pub trait McpHost: Send + Sync {
    /// Whether `name` is dispatched through the MCP pool (vs. the
    /// native [`ToolRegistry`](crate::engine::turn_loop::TurnLoopToolRegistry)).
    ///
    /// Default delegates to
    /// [`is_mcp_tool_name`](crate::engine::dispatch::is_mcp_tool_name).
    /// Cross-verified against `tui::mcp::McpPool::is_mcp_tool` by the
    /// `is_mcp_tool_name_matches_tui_mcp_pool` unit test in
    /// `crates/core/src/engine/dispatch.rs::tests` (and a mirrored
    /// `cross_verify_core_is_mcp_tool_name` test in `tui::mcp`).
    fn is_mcp_tool(&self, name: &str) -> bool {
        crate::engine::dispatch::is_mcp_tool_name(name)
    }

    /// Whether the MCP tool `name` may run inside the parallel batch
    /// executor (read-only + side-effect-free).
    ///
    /// Default delegates to
    /// [`mcp_tool_is_parallel_safe`](crate::engine::dispatch::mcp_tool_is_parallel_safe).
    fn tool_is_parallel_safe(&self, name: &str) -> bool {
        crate::engine::dispatch::mcp_tool_is_parallel_safe(name)
    }

    /// Whether the MCP tool `name` is read-only (skips the approval
    /// prompt for trusted-by-default operations like
    /// `read_mcp_resource`).
    ///
    /// Default delegates to
    /// [`mcp_tool_is_read_only`](crate::engine::dispatch::mcp_tool_is_read_only).
    fn tool_is_read_only(&self, name: &str) -> bool {
        crate::engine::dispatch::mcp_tool_is_read_only(name)
    }

    /// Human-readable approval prompt text for the MCP tool `name`.
    ///
    /// Default delegates to
    /// [`mcp_tool_approval_description`](crate::engine::dispatch::mcp_tool_approval_description).
    fn tool_approval_description(&self, name: &str) -> String {
        crate::engine::dispatch::mcp_tool_approval_description(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stand-in MCP host with no state — exercises default-impl paths
    /// without pulling the heavy `tui::mcp::McpPool` into core tests.
    struct StubMcpHost;
    impl McpHost for StubMcpHost {}

    #[test]
    fn default_impls_match_dispatch_module() {
        let host = StubMcpHost;
        assert!(host.is_mcp_tool("mcp_filesystem_read"));
        assert!(host.is_mcp_tool("list_mcp_resources"));
        assert!(!host.is_mcp_tool("read_file"));
        assert!(host.tool_is_read_only("read_mcp_resource"));
        assert!(host.tool_is_parallel_safe("list_mcp_resources"));
        assert!(!host.tool_is_read_only("mcp_filesystem_write"));
        let desc = host.tool_approval_description("read_mcp_resource");
        assert!(desc.starts_with("Read-only"));
    }

    #[test]
    fn dyn_dispatch_compiles() {
        fn _accepts(_: &dyn McpHost) {}
        _accepts(&StubMcpHost);
    }
}
