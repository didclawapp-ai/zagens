pub(crate) use context::compact_tool_result_for_context;
use context::{
    COMPACTION_SUMMARY_MARKER, MAX_CONTEXT_RECOVERY_ATTEMPTS, MIN_RECENT_MESSAGES_TO_KEEP,
    TURN_MAX_OUTPUT_TOKENS, context_input_budget, effective_max_output_tokens,
    estimate_input_tokens_conservative, extract_compaction_summary_prompt,
    is_context_length_error_message, summarize_text, turn_response_headroom_tokens,
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
use dispatch::{
    ParallelToolResult, ParallelToolResultEntry, ToolExecGuard, ToolExecOutcome, ToolExecutionPlan,
    caller_allowed_for_tool, caller_type_for_tool_use, final_tool_input, format_tool_error,
    mcp_tool_approval_description, mcp_tool_is_parallel_safe, mcp_tool_is_read_only,
    parse_parallel_tool_calls, parse_tool_input, should_force_update_plan_first,
    should_parallelize_tool_batch, should_stop_after_plan_tool,
};
use tool_catalog::{
    CODE_EXECUTION_TOOL_NAME, MULTI_TOOL_PARALLEL_NAME, REQUEST_USER_INPUT_NAME,
    active_tools_for_step, build_model_tool_catalog, ensure_advanced_tooling,
    execute_code_execution_tool, execute_tool_search, initial_active_tools, is_tool_search_tool,
    maybe_activate_requested_deferred_tool, missing_tool_error_message,
};
#[cfg(test)]
use tool_catalog::{TOOL_SEARCH_BM25_NAME, should_default_defer_tool};
use tool_execution::emit_tool_audit;

use approval::{ApprovalDecision, ApprovalResult, UserInputDecision};
use crate::prompts;
use crate::tools::subagent::resolve_subagent_assignment_route;
