use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config::MAX_SUBAGENTS;
use deepseek_core::events::Event;
use crate::models::{ContentBlock, Message, MessageRequest, SystemPrompt, Tool};
use crate::tools::plan::{PlanState, SharedPlanState};
use crate::tools::registry::{ToolRegistry, ToolRegistryBuilder};
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool, optional_u64, required_str,
};
use crate::tools::todo::{SharedTodoList, TodoList};
use crate::utils::spawn_supervised;

use super::blackboard::{read_blackboard_section, write_blackboard_partition};
use deepseek_core::subagent::{
    MailboxMessage, StructuredVerdict, SubAgentAssignment, SubAgentResult, SubAgentStatus,
    SubAgentType, VerdictLevel,
};
use super::mailbox::{Mailbox, MailboxEnvelope, MailboxReceiver};

use super::manager::SubAgentManager;
use super::constants::{SUBAGENT_STATE_FILE};


/// Thread-safe wrapper for `SubAgentManager`.
pub type SharedSubAgentManager = Arc<RwLock<SubAgentManager>>;

pub(crate) fn default_state_path(workspace: &Path) -> PathBuf {
    workspace
        .join(".deepseek")
        .join("state")
        .join(SUBAGENT_STATE_FILE)
}

pub(super) fn epoch_millis_now() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
        Err(_) => 0,
    }
}

pub(super) fn instant_from_duration(duration: Duration) -> Instant {
    Instant::now()
        .checked_sub(duration)
        .unwrap_or_else(Instant::now)
}

pub(super) fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(value)?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, payload)?;
    fs::rename(tmp_path, path)?;
    Ok(())
}

/// Create a shared sub-agent manager with a configurable limit.
#[must_use]
pub fn new_shared_subagent_manager(workspace: PathBuf, max_agents: usize) -> SharedSubAgentManager {
    let max_agents = max_agents.clamp(1, MAX_SUBAGENTS);
    let state_path = default_state_path(&workspace);
    let mut manager = SubAgentManager::new(workspace, max_agents).with_state_path(state_path);
    if let Err(err) = manager.load_state() {
        eprintln!("Failed to load sub-agent state: {err}");
    }
    Arc::new(RwLock::new(manager))
}

