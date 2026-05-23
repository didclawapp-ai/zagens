//! Structured sub-agent mailbox envelopes for engine events (P2 PR4).

use serde::{Deserialize, Serialize};

use crate::models::Usage;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MailboxMessage {
    Started {
        agent_id: String,
        agent_type: String,
    },
    Progress {
        agent_id: String,
        status: String,
    },
    ToolCallStarted {
        agent_id: String,
        tool_name: String,
        step: u32,
    },
    ToolCallCompleted {
        agent_id: String,
        tool_name: String,
        step: u32,
        ok: bool,
    },
    ChildSpawned {
        parent_id: String,
        child_id: String,
    },
    Completed {
        agent_id: String,
        summary: String,
    },
    Failed {
        agent_id: String,
        error: String,
    },
    Cancelled {
        agent_id: String,
    },
    TokenUsage {
        agent_id: String,
        model: String,
        usage: Usage,
    },
}

impl MailboxMessage {
    #[must_use]
    pub fn started(agent_id: impl Into<String>, agent_type: impl AsRef<str>) -> Self {
        Self::Started {
            agent_id: agent_id.into(),
            agent_type: agent_type.as_ref().to_string(),
        }
    }

    #[must_use]
    pub fn progress(agent_id: impl Into<String>, status: impl Into<String>) -> Self {
        Self::Progress {
            agent_id: agent_id.into(),
            status: status.into(),
        }
    }

    #[must_use]
    pub fn token_usage(
        agent_id: impl Into<String>,
        model: impl Into<String>,
        usage: Usage,
    ) -> Self {
        Self::TokenUsage {
            agent_id: agent_id.into(),
            model: model.into(),
            usage,
        }
    }

    #[must_use]
    pub fn agent_id(&self) -> &str {
        match self {
            Self::Started { agent_id, .. }
            | Self::Progress { agent_id, .. }
            | Self::ToolCallStarted { agent_id, .. }
            | Self::ToolCallCompleted { agent_id, .. }
            | Self::Completed { agent_id, .. }
            | Self::Failed { agent_id, .. }
            | Self::Cancelled { agent_id }
            | Self::TokenUsage { agent_id, .. } => agent_id,
            Self::ChildSpawned { child_id, .. } => child_id,
        }
    }
}
