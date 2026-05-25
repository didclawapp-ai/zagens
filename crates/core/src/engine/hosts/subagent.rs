//! [`SubAgentHost`] — engine boundary for background sub-agent spawn /
//! list / wait operations.
//!
//! M3 replaces the older
//! [`SubAgentSpawnPort`](crate::engine::subagent_port::SubAgentSpawnPort)
//! with a slightly broader trait that also exposes `running_count` (used
//! by `no_tool_uses.rs` to emit *"Waiting on N sub-agent(s)…"* status
//! events) and renames `list_subagents` → `list_with_cleanup` so the
//! name reflects what the implementation does (calls `manager.cleanup`
//! with the standard retention window before returning).
//!
//! `SubAgentSpawnPort` is kept as a `#[deprecated]` alias for one
//! release so existing imports keep building. New code should use
//! `SubAgentHost`.
//!
//! Method surface derived from the live `Engine`'s direct calls on the
//! `subagent_manager` field (M3 R1 mitigation):
//!
//! | File                                                             | Call                          |
//! |------------------------------------------------------------------|-------------------------------|
//! | `crates/tui/src/core/engine/subagent_spawn.rs:32–93`             | spawn orchestration → `spawn_general` |
//! | `crates/tui/src/core/engine/subagent_spawn.rs:95–99`             | cleanup + list → `list_with_cleanup` |
//! | `crates/tui/src/core/engine/turn_loop/host_impl/no_tool_uses.rs:68` | `mgr.running_count()` → `running_count` |
//!
//! The bare `Arc<RwLock<SubAgentManager>>` is still passed directly into
//! `ToolContext` / `SubAgentRuntime` / `StructuredState::capture`; that
//! wire is **not** abstracted in M3 (would force tools-crate churn).

use async_trait::async_trait;

use crate::engine::subagent_port::{SubAgentSpawnError, SubAgentSpawnOutcome};
use crate::subagent::SubAgentResult;

/// Engine-side sub-agent host.
///
/// Implemented by `crates/tui/src/core/engine/subagent_spawn.rs`'s
/// `impl SubAgentHost for Engine` — the spawn orchestration touches
/// multiple Engine fields (`deepseek_client`, `session.model`,
/// `tx_event`, …) so the trait must impl on `Engine` itself rather
/// than on the underlying `SubAgentManager`.
#[async_trait]
pub trait SubAgentHost: Send + Sync {
    /// Spawn a background sub-agent of the `General` type. Returns the
    /// new agent's id on success. Surfaces `NoClient` when the API
    /// client is not configured and `SpawnFailed(msg)` for all other
    /// orchestration failures (depth cap, manager rejection, …).
    async fn spawn_general(
        &self,
        prompt: &str,
    ) -> Result<SubAgentSpawnOutcome, SubAgentSpawnError>;

    /// List all sub-agents held by the manager. Calls `manager.cleanup`
    /// with the standard retention window before returning so stale
    /// entries are pruned in the same op-handler hop.
    async fn list_with_cleanup(&self) -> Vec<SubAgentResult>;

    /// Currently-running sub-agent count. Used by `no_tool_uses.rs` to
    /// emit "Waiting on N sub-agent(s) to complete..." status events
    /// when the model produced no tool calls but background agents are
    /// still in flight.
    async fn running_count(&self) -> usize;
}
