//! Shared imports for engine submodules (`use super::*` legacy pattern).

#![allow(unused_imports, reason = "submodules use `super::*` from engine via this prelude")]

pub use std::collections::HashMap;
pub use std::collections::hash_map::DefaultHasher;
pub use std::hash::{Hash, Hasher};
pub use std::path::PathBuf;
pub use std::sync::{Arc, Mutex as StdMutex};
pub use std::time::{Duration, Instant};

pub use anyhow::Result;
pub use futures_util::StreamExt;
pub use futures_util::stream::FuturesUnordered;
pub use serde_json::json;
pub use tokio::sync::{Mutex as AsyncMutex, RwLock, mpsc};
pub use tokio_util::sync::CancellationToken;

pub use crate::client::DeepSeekClient;
pub use crate::compaction::{
    compact_messages_safe, merge_system_prompts, should_compact, CompactionConfig,
};
pub use crate::config::{ApiProvider, Config, DEFAULT_MAX_SUBAGENTS, DEFAULT_TEXT_MODEL};
pub use crate::cycle_manager::{
    archive_cycle, build_seed_messages, estimate_briefing_tokens, produce_briefing,
    should_advance_cycle, CycleBriefing, CycleConfig, StructuredState,
};
pub use crate::error_taxonomy::{ErrorCategory, ErrorEnvelope};
pub use crate::features::{Feature, Features};
pub use crate::llm_client::LlmClient;
pub use crate::mcp::McpPool;
#[cfg(test)]
pub use crate::models::ToolCaller;
pub use crate::models::{
    ContentBlock, ContentBlockStart, Delta, Message, MessageRequest, StreamEvent, SystemPrompt,
    Tool, Usage, LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS,
};
use crate::prompts;
pub use crate::seam_manager::{SeamConfig, SeamManager};
pub use crate::tools::plan::{new_shared_plan_state, SharedPlanState};
pub use crate::tools::shell::{new_shared_shell_manager, SharedShellManager};
pub use crate::tools::spec::RuntimeToolServices;
pub use crate::tools::spec::{ApprovalRequirement, ToolError, ToolResult};
pub use crate::tools::subagent::{
    new_shared_subagent_manager, Mailbox, SharedSubAgentManager, SubAgentCompletion,
    SubAgentRuntime, SubAgentType,
};
use crate::tools::subagent::resolve_subagent_assignment_route;
pub use crate::tools::todo::{new_shared_todo_list, SharedTodoList};
pub use crate::tools::user_input::{UserInputRequest, UserInputResponse};
pub use crate::tools::{ToolContext, ToolRegistryBuilder};
pub use crate::agent_surface::AppMode;

pub use crate::core::capacity::{
    CapacityController, CapacityControllerConfig, CapacityDecision, CapacityObservationInput,
    CapacitySnapshot, GuardrailAction, RiskBand,
};
pub use crate::core::capacity_memory::{
    append_capacity_record, load_last_k_capacity_records, new_record_id, now_rfc3339,
    CanonicalState, CapacityMemoryRecord, ReplayInfo,
};
pub use crate::core::coherence::{next_coherence_state, CoherenceSignal, CoherenceState};
pub use crate::core::events::{Event, TurnOutcomeStatus};
pub use crate::core::ops::Op;
pub use crate::core::session::Session;
pub use crate::core::tool_parser;
pub use crate::core::turn::{post_turn_snapshot, pre_turn_snapshot, TurnContext, TurnToolCall};
