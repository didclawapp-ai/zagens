//! Engine boundary (P2 PR2–PR3).
//!
//! `Session` and related state live here. The live `Engine` / `turn_loop`
//! implementation remains in `deepseek-tui::core::engine` until PR4 thins the
//! shell wrapper (see `docs/tech/adr/P2_MIGRATION_SPIKE.md`).
//!
//! **PR3:** `StartTurnParams` + `TurnEnginePort` — `RuntimeThreadManager::start_turn`
//! validates and delegates through core before sending `Op::SendMessage`.

pub mod approval;
pub mod config;
pub mod context;
pub mod context_snapshot;
pub mod dispatch;
pub mod handle;
pub mod hosts;
pub mod op;
pub mod loop_guard;
pub mod lsp_edit_paths;
pub mod tool_bridge;
pub mod tool_catalog;
pub mod tool_progress;
pub mod turn_loop;
pub mod start_turn;
pub mod streaming;
pub mod tool_parser;
pub mod subagent_port;
mod tool_dispatch;
mod turn_port;

pub use approval::{
    await_tool_approval, recv_user_input_for_tool, ApprovalDecision, ApprovalResult,
    UserInputDecision,
};
pub use context::{
    compact_tool_result_for_context, context_input_budget, effective_max_output_tokens,
    estimate_input_tokens_conservative, extract_compaction_summary_prompt,
    is_context_length_error_message, summarize_text, turn_response_headroom_tokens,
    COMPACTION_SUMMARY_MARKER, MAX_CONTEXT_RECOVERY_ATTEMPTS, MIN_RECENT_MESSAGES_TO_KEEP,
    TURN_MAX_OUTPUT_TOKENS, count_oldest_messages_to_drain,
};
pub use context_snapshot::ThreadContextSnapshot;
pub use handle::EngineHandle;
pub use op::Op;
pub use dispatch::{
    caller_allowed_for_tool, caller_type_for_tool_use, final_tool_input, format_tool_error,
    is_mcp_tool_name, mcp_tool_approval_description, mcp_tool_is_parallel_safe,
    mcp_tool_is_read_only, parse_parallel_tool_calls, parse_tool_input_json,
    should_force_update_plan_first, should_parallelize_tool_batch, should_stop_after_plan_tool,
    ToolParallelPlanFlags,
};
pub use loop_guard::{AttemptDecision, LoopGuard, OutcomeDecision};
pub use lsp_edit_paths::{edited_paths_for_tool, parse_patch_paths};
pub use start_turn::StartTurnParams;
pub use streaming::{
    contains_fake_tool_wrapper, filter_tool_call_delta, should_transparently_retry_stream,
    ContentBlockKind, ToolUseState, FAKE_WRAPPER_NOTICE, MAX_STREAM_ERRORS_BEFORE_FAIL,
    MAX_TRANSPARENT_STREAM_RETRIES, STREAM_CHUNK_TIMEOUT_SECS, STREAM_MAX_CONTENT_BYTES,
    STREAM_MAX_DURATION_SECS, TOOL_CALL_END_MARKERS, TOOL_CALL_START_MARKERS,
};
pub use tool_bridge::{
    function_call_to_tool_error, tool_call_input, tool_name_is_mutating, tool_output_to_result,
    tool_result_to_output, value_to_tool_call,
};
pub use tool_catalog::{
    active_tools_for_step, apply_mcp_tool_deferral, apply_native_tool_deferral,
    build_model_tool_catalog, ensure_advanced_tooling, execute_tool_search, initial_active_tools,
    is_tool_search_tool, maybe_activate_requested_deferred_tool, missing_tool_error_message,
    should_default_defer_tool, CODE_EXECUTION_TOOL_NAME, MULTI_TOOL_PARALLEL_NAME,
    REQUEST_USER_INPUT_NAME, TOOL_SEARCH_BM25_NAME,
};
pub use tool_dispatch::EngineToolDispatch;
pub use tool_progress::{
    emit_tool_audit, tool_progress_opening_line, tool_progress_phase_line,
};
pub use turn_loop::{
    build_edit_file_approval_desc, handle_deepseek_turn, messages_with_turn_metadata,
    resolve_auto_effort, ToolExecOutcome, ToolExecutionPlan, ToolPlanApprovalMeta,
    TurnLoopConfigView, TurnLoopControl,
    TurnLoopHost, TurnLoopStreamingPhaseOutcome, TurnLoopToolExec, TurnLoopToolExecutor,
    TurnLoopToolPhaseOutcome, McpPoolPort, TurnLoopToolRegistry,
};
pub use turn_port::TurnEnginePort;
pub use subagent_port::{SubAgentSpawnError, SubAgentSpawnOutcome};
#[allow(deprecated)]
pub use subagent_port::SubAgentSpawnPort;
pub use hosts::{LspHost, McpHost, SandboxHost, ShellHost, SubAgentHost};
pub use crate::turn::{TurnContext, TurnLoopMode, TurnOutcomeStatus};
pub use crate::session::{Session, SessionUsage};
