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

pub(crate) const DEFAULT_MAX_STEPS: u32 = 100;
pub(crate) const TOOL_TIMEOUT: Duration = Duration::from_secs(30);
/// Per-step LLM API call timeout. Each `create_message` request must complete
/// within this window or the step is treated as timed out. Prevents a single
/// stuck API call from blocking the sub-agent indefinitely.
pub(crate) const STEP_API_TIMEOUT: Duration = Duration::from_secs(120);

pub(crate) fn step_api_timeout_error(secs: u64) -> anyhow::Error {
    anyhow!(
        "API call timed out after {secs}s (per-step cap). Child stopped — not proof the area is \
         fully reviewed. Parent: re-spawn with a smaller scope and step_timeout_ms=240000–360000, \
         raise [subagents] step_timeout_secs in config/settings, or continue with parallel \
         read_file; do not mark scratchpad inventory done on timeout alone."
    )
}
pub(crate) const RESULT_POLL_INTERVAL: Duration = Duration::from_millis(250);
pub(crate) const DEFAULT_RESULT_TIMEOUT_MS: u64 = 30_000;
pub(crate) const MIN_WAIT_TIMEOUT_MS: u64 = 10_000;
pub(crate) const MAX_RESULT_TIMEOUT_MS: u64 = 3_600_000;
pub(crate) const COMPLETED_AGENT_RETENTION: Duration = Duration::from_secs(60 * 60);
pub(crate) const SUBAGENT_STATE_SCHEMA_VERSION: u32 = 1;
pub(crate) const SUBAGENT_STATE_FILE: &str = "subagents.v1.json";
pub(crate) const SUBAGENT_RESTART_REASON: &str = "Interrupted by process restart";

pub(crate) const VALID_SUBAGENT_TYPES: &str = "general, explore, plan, review, implementer, verifier, custom, \
     worker, explorer, awaiter, default, implement, builder, verify, validator, tester";
/// Whale species names rotated through `whale_nickname_for_index` to label
/// sub-agents in the UI. English and Simplified-Chinese names are interleaved
/// so any newly spawned agent has a roughly even chance of either — the goal
/// is friendly variety, not a strict locale match.
pub const WHALE_NICKNAMES: &[&str] = &[
    "Blue",
    "蓝鲸",
    "Humpback",
    "座头鲸",
    "Sperm",
    "抹香鲸",
    "Fin",
    "长须鲸",
    "Sei",
    "塞鲸",
    "Bryde's",
    "布氏鲸",
    "Minke",
    "小须鲸",
    "Antarctic Minke",
    "南极小须鲸",
    "Gray",
    "灰鲸",
    "Bowhead",
    "弓头鲸",
    "North Atlantic Right",
    "北大西洋露脊鲸",
    "North Pacific Right",
    "北太平洋露脊鲸",
    "Southern Right",
    "南露脊鲸",
    "Beluga",
    "白鲸",
    "Narwhal",
    "独角鲸",
    "Orca",
    "虎鲸",
    "Pilot",
    "领航鲸",
    "False Killer",
    "伪虎鲸",
    "Pygmy Killer",
    "小虎鲸",
    "Melon-headed",
    "瓜头鲸",
    "Beaked",
    "喙鲸",
    "Cuvier's Beaked",
    "柯氏喙鲸",
    "Baird's Beaked",
    "贝氏喙鲸",
    "Blainville's Beaked",
    "柏氏喙鲸",
];

/// Removal version for deprecated tool aliases.
pub(crate) const DEPRECATION_REMOVAL_VERSION: &str = "0.8.0";

#[must_use]
pub fn whale_nickname_for_index(index: usize) -> String {
    let base = WHALE_NICKNAMES[index % WHALE_NICKNAMES.len()];
    if index < WHALE_NICKNAMES.len() {
        base.to_string()
    } else {
        format!("{base} {}", index / WHALE_NICKNAMES.len() + 1)
    }
}

