//! Core engine for `DeepSeek` CLI.
//!
//! The engine handles all AI interactions in a background task,
//! communicating with the UI via channels. This enables:
//! - Non-blocking UI during API calls
//! - Real-time streaming updates
//! - Proper cancellation support
//! - Tool execution orchestration

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use serde_json::json;
use tokio::sync::{Mutex as AsyncMutex, RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::client::DeepSeekClient;
use crate::compaction::{
    CompactionConfig, compact_messages_safe, merge_system_prompts, should_compact,
};
use crate::config::{ApiProvider, Config, DEFAULT_MAX_SUBAGENTS, DEFAULT_TEXT_MODEL};
use crate::cycle_manager::{
    CycleBriefing, CycleConfig, StructuredState, archive_cycle, build_seed_messages,
    estimate_briefing_tokens, produce_briefing, should_advance_cycle,
};
use crate::error_taxonomy::{ErrorCategory, ErrorEnvelope, StreamError};
use crate::features::{Feature, Features};
use crate::llm_client::LlmClient;
use crate::mcp::McpPool;
#[cfg(test)]
use crate::models::ToolCaller;
use crate::models::{
    ContentBlock, ContentBlockStart, Delta, LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS, Message,
    MessageRequest, StreamEvent, SystemPrompt, Tool, Usage,
};
use crate::prompts;
use crate::seam_manager::{SeamConfig, SeamManager};
use crate::tools::plan::{SharedPlanState, new_shared_plan_state};
use crate::tools::shell::{SharedShellManager, new_shared_shell_manager};
use crate::tools::spec::RuntimeToolServices;
use crate::tools::spec::{ApprovalRequirement, ToolError, ToolResult};
use crate::tools::subagent::{
    Mailbox, SharedSubAgentManager, SubAgentCompletion, SubAgentRuntime, SubAgentType,
    new_shared_subagent_manager, resolve_subagent_assignment_route,
};
use crate::tools::todo::{SharedTodoList, new_shared_todo_list};
use crate::tools::user_input::{UserInputRequest, UserInputResponse};
use crate::tools::{ToolContext, ToolRegistryBuilder};
use crate::tui::app::AppMode;
use crate::utils::spawn_supervised;

use super::capacity::{
    CapacityController, CapacityControllerConfig, CapacityDecision, CapacityObservationInput,
    CapacitySnapshot, GuardrailAction, RiskBand,
};
use super::capacity_memory::{
    CanonicalState, CapacityMemoryRecord, ReplayInfo, append_capacity_record,
    load_last_k_capacity_records, new_record_id, now_rfc3339,
};
use super::coherence::{CoherenceSignal, CoherenceState, next_coherence_state};
use super::events::{Event, TurnOutcomeStatus};
use super::ops::Op;
use super::session::Session;
use super::tool_parser;
use super::turn::{TurnContext, TurnToolCall, post_turn_snapshot, pre_turn_snapshot};

mod approval;
mod types;
mod handle;
mod engine_helpers;
mod engine_new;
mod session_messages;
#[cfg(test)]
mod mock;

pub use types::EngineConfig;
pub use handle::EngineHandle;

/// The core engine that processes operations and emits events
pub struct Engine {
    config: EngineConfig,
    deepseek_client: Option<Arc<dyn crate::llm_client::LlmClient>>,
    deepseek_client_error: Option<String>,
    api_key_env_only_recovery: Option<String>,
    session: Session,
    subagent_manager: SharedSubAgentManager,
    shell_manager: SharedShellManager,
    mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
    rx_op: mpsc::Receiver<Op>,
    tx_approval: mpsc::Sender<ApprovalDecision>,
    rx_approval: mpsc::Receiver<ApprovalDecision>,
    rx_user_input: mpsc::Receiver<UserInputDecision>,
    rx_steer: mpsc::Receiver<String>,
    tx_event: mpsc::Sender<Event>,
    tx_subagent_completion: mpsc::UnboundedSender<SubAgentCompletion>,
    pub(super) rx_subagent_completion: mpsc::UnboundedReceiver<SubAgentCompletion>,
    cancel_token: CancellationToken,
    shared_cancel_token: Arc<StdMutex<CancellationToken>>,
    tool_exec_lock: Arc<RwLock<()>>,
    capacity_controller: CapacityController,
    seam_manager: Option<SeamManager>,
    coherence_state: CoherenceState,
    turn_counter: u64,
    lsp_manager: Arc<crate::lsp::LspManager>,
    workshop_vars: Option<
        std::sync::Arc<tokio::sync::Mutex<crate::tools::large_output_router::WorkshopVariables>>,
    >,
    sandbox_backend: Option<std::sync::Arc<dyn crate::sandbox::backend::SandboxBackend>>,
    pending_lsp_blocks: Vec<crate::lsp::DiagnosticBlock>,
    scratchpad_step: scratchpad_flow::ScratchpadStepState,
    scratchpad_run_id: Option<String>,
    scratchpad_summary_injected_this_turn: bool,
    topic_memory_runtime: crate::topic_memory::TopicMemoryRuntime,
}

use self::approval::{ApprovalDecision, UserInputDecision};

/// Spawn the engine in a background task
pub fn spawn_engine(config: EngineConfig, api_config: &Config) -> EngineHandle {
    let (engine, handle) = Engine::new(config, api_config);

    spawn_supervised(
        "engine-event-loop",
        std::panic::Location::caller(),
        async move {
            engine.run().await;
        },
    );

    handle
}

#[cfg(test)]
pub(crate) use mock::{MockApprovalEvent, MockEngineHandle, mock_engine_handle};

mod turn_port;
mod capacity_flow;
mod context_recovery;
mod context_trim;
mod layered_context;
mod tool_context;
mod cycle_hooks;
mod message_handlers;
mod edit_turn_ops;
mod compaction_ops;
mod op_loop;
mod session_ops;
mod context;
pub(crate) use context::compact_tool_result_for_context;
use context::{
    COMPACTION_SUMMARY_MARKER, MAX_CONTEXT_RECOVERY_ATTEMPTS, MIN_RECENT_MESSAGES_TO_KEEP,
    TURN_MAX_OUTPUT_TOKENS, context_input_budget, effective_max_output_tokens,
    estimate_input_tokens_conservative, extract_compaction_summary_prompt,
    is_context_length_error_message, summarize_text, turn_response_headroom_tokens,
};
mod dispatch;
mod loop_guard;
mod lsp_hooks;
mod streaming;
mod tool_catalog;
mod tool_execution;
mod tool_setup;
mod tool_dispatch_port;
pub mod scratchpad_flow;
mod subagent_spawn;
mod op_handlers;
mod turn_loop;

pub(crate) use tool_dispatch_port::{RegistryToolDispatch, TuiEngineToolDispatch};

use self::approval::ApprovalResult;
use self::dispatch::{
    ParallelToolResult, ParallelToolResultEntry, ToolExecGuard, ToolExecOutcome, ToolExecutionPlan,
    caller_allowed_for_tool, caller_type_for_tool_use, final_tool_input, format_tool_error,
    mcp_tool_approval_description, mcp_tool_is_parallel_safe, mcp_tool_is_read_only,
    parse_parallel_tool_calls, parse_tool_input, should_force_update_plan_first,
    should_parallelize_tool_batch, should_stop_after_plan_tool,
};
use deepseek_core::engine::SubAgentSpawnError;
use deepseek_core::engine::loop_guard::{AttemptDecision, LoopGuard, OutcomeDecision};
#[cfg(test)]
use deepseek_core::engine::{edited_paths_for_tool, parse_patch_paths};
#[cfg(test)]
use deepseek_core::engine::streaming::TOOL_CALL_START_MARKERS;
use deepseek_core::engine::streaming::{
    ContentBlockKind, FAKE_WRAPPER_NOTICE, MAX_STREAM_ERRORS_BEFORE_FAIL,
    MAX_TRANSPARENT_STREAM_RETRIES, STREAM_CHUNK_TIMEOUT_SECS, STREAM_MAX_CONTENT_BYTES,
    STREAM_MAX_DURATION_SECS, ToolUseState, contains_fake_tool_wrapper, filter_tool_call_delta,
    should_transparently_retry_stream,
};
use self::tool_catalog::{
    CODE_EXECUTION_TOOL_NAME, MULTI_TOOL_PARALLEL_NAME, REQUEST_USER_INPUT_NAME,
    active_tools_for_step, build_model_tool_catalog, ensure_advanced_tooling,
    execute_code_execution_tool, execute_tool_search, initial_active_tools, is_tool_search_tool,
    maybe_activate_requested_deferred_tool, missing_tool_error_message,
};
#[cfg(test)]
use self::tool_catalog::{TOOL_SEARCH_BM25_NAME, should_default_defer_tool};
use self::tool_execution::emit_tool_audit;

#[cfg(test)]
mod tests;
