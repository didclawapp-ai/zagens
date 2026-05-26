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

use super::constants::DEPRECATION_REMOVAL_VERSION;

// === Deprecation helpers ===

/// Wrap a `ToolResult` with a `_deprecation` block in its metadata.
///
/// Applied exclusively on alias paths (not on canonical tool names) so the
/// model can detect and migrate away from the old name before removal in
/// v`DEPRECATION_REMOVAL_VERSION`.
///
/// The `_deprecation` key is merged into any existing metadata so other
/// metadata (e.g. `status`, `timed_out`) is preserved unchanged.
pub(crate) fn wrap_with_deprecation_notice(
    mut result: ToolResult,
    this_tool: &str,
    use_instead: &str,
) -> ToolResult {
    tracing::warn!(
        "Deprecated tool '{}' invoked — use '{}' instead (removal: v{})",
        this_tool,
        use_instead,
        DEPRECATION_REMOVAL_VERSION,
    );

    let notice = json!({
        "_deprecation": {
            "this_tool": this_tool,
            "use_instead": use_instead,
            "removed_in": DEPRECATION_REMOVAL_VERSION,
            "message": format!(
                "Tool '{}' is deprecated; switch to '{}' before v{}.",
                this_tool, use_instead, DEPRECATION_REMOVAL_VERSION
            )
        }
    });

    result.metadata = Some(match result.metadata.take() {
        Some(Value::Object(mut map)) => {
            if let Value::Object(notice_map) = notice {
                map.extend(notice_map);
            }
            Value::Object(map)
        }
        Some(other) => {
            // Existing metadata was not an object — keep it as-is and add
            // the deprecation notice as a sibling under a wrapper.
            json!({ "_deprecation": notice["_deprecation"].clone(), "_original_metadata": other })
        }
        None => notice,
    });

    result
}

