//! Sub-agent spawn port for the engine op loop (P2 tool_execution portization).

use async_trait::async_trait;

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

/// Background sub-agent spawn/list surface (L2: `deepseek-tui` implements).
#[async_trait]
pub trait SubAgentSpawnPort: Send + Sync {
    async fn spawn_general_subagent(
        &self,
        prompt: &str,
    ) -> Result<SubAgentSpawnOutcome, SubAgentSpawnError>;
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
