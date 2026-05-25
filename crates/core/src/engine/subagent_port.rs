//! Sub-agent spawn outcome / error types (P2 tool_execution portization).
//!
//! M3 (Engine-struct strangler) replaced the original `SubAgentSpawnPort`
//! trait with [`SubAgentHost`](crate::engine::hosts::SubAgentHost), which
//! covers the same spawn / list surface plus `running_count` and uses the
//! clearer `list_with_cleanup` method name. The old `SubAgentSpawnPort`
//! trait is preserved as a deprecated re-export shim
//! (`pub trait SubAgentSpawnPort = SubAgentHost;`-flavored, via blanket
//! impl) for one release so existing imports keep building.

use async_trait::async_trait;

use crate::subagent::SubAgentResult;

/// Outcome of spawning a background sub-agent from the engine op loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubAgentSpawnOutcome {
    pub agent_id: String,
}

/// Errors surfaced when sub-agent spawn cannot proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubAgentSpawnError {
    NoClient,
    SpawnFailed(String),
}

impl std::fmt::Display for SubAgentSpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoClient => write!(f, "API client not configured"),
            Self::SpawnFailed(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for SubAgentSpawnError {}

/// Deprecated alias for [`SubAgentHost`](crate::engine::hosts::SubAgentHost).
///
/// Kept for one release after the M3 rename
/// (`SubAgentSpawnPort` → `SubAgentHost`). The new trait keeps the
/// `spawn_general_subagent` / `list_subagents` methods so existing impls
/// (e.g. `impl SubAgentSpawnPort for Engine` in tui) continue to compile
/// without behavior change; new code should impl `SubAgentHost` directly
/// and use `spawn_general` / `list_with_cleanup` / `running_count`.
#[deprecated(
    since = "0.8.16",
    note = "use `deepseek_core::engine::hosts::SubAgentHost` instead; \
            this alias will be removed in the next release"
)]
#[async_trait]
pub trait SubAgentSpawnPort: Send + Sync {
    async fn spawn_general_subagent(
        &self,
        prompt: &str,
    ) -> Result<SubAgentSpawnOutcome, SubAgentSpawnError>;

    /// List all sub-agents held by the manager (includes prior-session entries).
    async fn list_subagents(&self) -> Vec<SubAgentResult>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subagent_spawn_error_display_messages() {
        assert_eq!(
            SubAgentSpawnError::NoClient.to_string(),
            "API client not configured"
        );
        assert_eq!(
            SubAgentSpawnError::SpawnFailed("depth limit".into()).to_string(),
            "depth limit"
        );
    }
}
