use std::path::PathBuf;

use serde::{Deserialize, Serialize};


use deepseek_core::subagent::{
    SubAgentAssignment, SubAgentResult, SubAgentStatus,
    SubAgentType,
};

use super::constants::SUBAGENT_STATE_SCHEMA_VERSION;

#[derive(Debug, Clone, Default)]
pub(crate) struct SubAgentSpawnOptions {
    pub model: Option<String>,
    pub nickname: Option<String>,
    /// Optional task id for blackboard association (CRAFT P1).
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WaitMode {
    Any,
    All,
}

impl WaitMode {
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "any" | "first" => Some(Self::Any),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::All => "all",
        }
    }

    pub(crate) fn condition_met(self, snapshots: &[SubAgentResult]) -> bool {
        match self {
            Self::Any => snapshots
                .iter()
                .any(|snapshot| snapshot.status != SubAgentStatus::Running),
            Self::All => snapshots
                .iter()
                .all(|snapshot| snapshot.status != SubAgentStatus::Running),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SubAgentInput {
    pub(crate) text: String,
    pub(crate) interrupt: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SpawnRequest {
    pub(crate) prompt: String,
    pub(crate) agent_type: SubAgentType,
    pub(crate) assignment: SubAgentAssignment,
    pub(crate) allowed_tools: Option<Vec<String>>,
    pub(crate) model: Option<String>,
    /// Optional working directory for the child. Must canonicalize to a
    /// path inside the parent's workspace. Used to dispatch parallel work
    /// into separate git worktrees: parent runs `git worktree add` first,
    /// then spawns children with the worktree path as `cwd`.
    pub(crate) cwd: Option<PathBuf>,
    /// Optional file path for cache-aware resident mode (#529). When set,
    /// the child's prompt is prefixed with the file contents for prefix-cache
    /// locality. A global ownership table prevents two agents from holding
    /// a resident lease on the same file simultaneously.
    pub(crate) resident_file: Option<String>,
    /// Optional task id for blackboard association (CRAFT P1).
    /// When set, the child reads/writes `.deepseek/blackboards/{task_id}.json`.
    pub(crate) task_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AssignRequest {
    pub(crate) agent_id: String,
    pub(crate) objective: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) interrupt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistedSubAgent {
    pub(crate) id: String,
    pub(crate) agent_type: SubAgentType,
    pub(crate) prompt: String,
    pub(crate) assignment: SubAgentAssignment,
    #[serde(default)]
    pub(crate) model: String,
    #[serde(default)]
    pub(crate) nickname: Option<String>,
    pub(crate) status: SubAgentStatus,
    pub(crate) result: Option<String>,
    pub(crate) steps_taken: u32,
    pub(crate) duration_ms: u64,
    pub(crate) allowed_tools: Vec<String>,
    pub(crate) updated_at_ms: u64,
    /// Stable id of the manager / process boot that spawned this agent
    /// (#405). Lets a fresh manager filter out agents that were
    /// persisted by a prior session. Optional with `#[serde(default)]`
    /// for backward compatibility — older records lack the field and
    /// load with an empty string, which the manager treats as
    /// "from_prior_session" because it can't match any current id.
    #[serde(default)]
    pub(crate) session_boot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistedSubAgentState {
    pub(crate) schema_version: u32,
    pub(crate) agents: Vec<PersistedSubAgent>,
}

impl Default for PersistedSubAgentState {
    fn default() -> Self {
        Self {
            schema_version: SUBAGENT_STATE_SCHEMA_VERSION,
            agents: Vec::new(),
        }
    }
}

/// Default cap on sub-agent recursion depth. Override via
/// `[runtime] max_spawn_depth = N` in `~/.deepseek/config.toml`.
pub const DEFAULT_MAX_SPAWN_DEPTH: u32 = 3;

