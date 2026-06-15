//! Session and turn loop boundaries (P2 PR2–PR4).
//!
//! `Session` and related state live here. The live `Engine` / `turn_loop`
//! implementation is in `zagens-core`; the sidecar provides host adapters.
//!
//! **PR3:** `StartTurnParams` + `TurnEnginePort` — `RuntimeThreadManager::start_turn`
//! validates and delegates through core before sending `Op::SendMessage`.

pub mod approval;
pub mod config;
pub mod context;
pub mod context_compiler;
pub mod context_snapshot;
pub mod dispatch;
pub mod handle;
pub mod host_bundle;
pub mod hosts;
pub mod kernel_event;
pub mod kernel_event_golden;
pub mod kernel_mode;
pub mod kernel_turn_host;
pub mod loop_guard;
pub mod lsp_edit_paths;
pub mod op;
pub mod op_loop;
pub mod platform_ext;
pub mod request_fingerprint;
pub mod runtime;
pub mod runtime_new;
pub mod scratchpad_state;
pub mod start_turn;
pub mod streaming;
pub mod subagent_port;
pub mod token_estimate;
pub mod tool_bridge;
pub mod tool_catalog;
mod tool_dispatch;
pub mod tool_effects;
pub mod tool_parser;
pub mod tool_progress;
pub mod turn_loop;
pub mod turn_machine;
mod turn_port;

pub use crate::session::{Session, SessionUsage};
pub use crate::turn::{TurnContext, TurnLoopMode, TurnOutcomeStatus};
pub use approval::{
    ApprovalDecision, ApprovalResult, UserInputDecision, await_tool_approval,
    recv_user_input_for_tool,
};
pub use context::{
    COMPACTION_SUMMARY_MARKER, MAX_CONTEXT_RECOVERY_ATTEMPTS, MIN_RECENT_MESSAGES_TO_KEEP,
    TURN_MAX_OUTPUT_TOKENS, compact_tool_result_for_context, context_input_budget,
    count_oldest_messages_to_drain, effective_max_output_tokens,
    estimate_input_tokens_conservative, extract_compaction_summary_prompt,
    is_context_length_error_message, summarize_text, turn_response_headroom_tokens,
};
pub use context_compiler::{
    BudgetOverride, BudgetPolicy, CompileError, CompiledContext, ContextCompiler,
    ContextCompilerMode, ContextLayer, ContextProjection, ContextSource, RenderFn, RenderedBlock,
    SourceContribution, SourceId,
};
pub use context_snapshot::ThreadContextSnapshot;
pub use dispatch::{
    ToolParallelPlanFlags, caller_allowed_for_tool, caller_type_for_tool_use, final_tool_input,
    format_tool_error, is_mcp_tool_name, mcp_tool_approval_description, mcp_tool_is_parallel_safe,
    mcp_tool_is_read_only, parse_parallel_tool_calls, parse_tool_input_json,
    should_force_update_plan_first, should_parallelize_tool_batch, should_stop_after_plan_tool,
};
pub use handle::EngineHandle;
pub use host_bundle::EngineHostBundle;
pub use hosts::{
    LspHost, McpHost, SandboxHost, SeamError, SeamHost, ShellHost, SubAgentHost, TopicMemoryHost,
    WorkshopHost,
};
pub use kernel_event::{
    ArtifactId, CallId, CapacityAction, CapacityCheckpointKind, DeltaKind, KernelEvent,
    KernelEventEnvelope, MessageRange, OverflowStrategy, PolicyDecision, ToolOutcome, TurnId,
    TurnOutcome,
};
pub use kernel_mode::KernelMachineMode;
pub use kernel_turn_host::KernelTurnHost;
pub use loop_guard::{AttemptDecision, LoopGuard, OutcomeDecision};
pub use lsp_edit_paths::{edited_paths_for_tool, parse_patch_paths};
pub use op::Op;
pub use request_fingerprint::{RequestFingerprint, compute_request_fingerprint, sha256_hex};
pub use runtime::Engine;
pub use scratchpad_state::ScratchpadStepState;
pub use start_turn::StartTurnParams;
pub use streaming::{
    ContentBlockKind, FAKE_WRAPPER_NOTICE, MAX_STREAM_ERRORS_BEFORE_FAIL,
    MAX_TRANSPARENT_STREAM_RETRIES, STREAM_CHUNK_TIMEOUT_SECS, STREAM_MAX_CONTENT_BYTES,
    STREAM_MAX_DURATION_SECS, TOOL_CALL_END_MARKERS, TOOL_CALL_START_MARKERS, ToolUseState,
    contains_fake_tool_wrapper, filter_tool_call_delta, should_transparently_retry_stream,
};
#[allow(deprecated)]
pub use subagent_port::SubAgentSpawnPort;
pub use subagent_port::{SubAgentSpawnError, SubAgentSpawnOutcome};
pub use token_estimate::{
    MESSAGE_FRAMING_TOKENS, SESSION_FRAMING_TOKENS, TokenEstimator, estimate_text_tokens,
};
pub use tool_bridge::{
    function_call_to_tool_error, tool_call_input, tool_name_is_mutating, tool_output_to_result,
    tool_result_to_output, value_to_tool_call,
};
pub use tool_catalog::{
    CODE_EXECUTION_TOOL_NAME, MULTI_TOOL_PARALLEL_NAME, REQUEST_USER_INPUT_NAME,
    TOOL_SEARCH_BM25_NAME, active_tools_for_step, apply_mcp_tool_deferral,
    apply_native_tool_deferral, build_model_tool_catalog, ensure_advanced_tooling,
    execute_tool_search, initial_active_tools, is_tool_search_tool,
    maybe_activate_requested_deferred_tool, missing_tool_error_message, should_default_defer_tool,
};
pub use tool_dispatch::EngineToolDispatch;
pub use tool_effects::tool_writes_state;
pub use tool_progress::{emit_tool_audit, tool_progress_opening_line, tool_progress_phase_line};
pub use turn_loop::{
    McpPoolPort, ToolExecOutcome, ToolExecutionPlan, ToolPlanApprovalMeta, TurnLoopConfigView,
    TurnLoopControl, TurnLoopHost, TurnLoopStreamingPhaseOutcome, TurnLoopToolExec,
    TurnLoopToolExecutor, TurnLoopToolPhaseOutcome, TurnLoopToolRegistry,
    build_edit_file_approval_desc, handle_deepseek_turn, messages_with_turn_metadata,
    resolve_auto_effort,
};
pub use turn_machine::{
    Effect, KernelEventSink, KernelResumeHints, LiveTurnSnapshot, ReplayTurnMachine,
    SessionMessageCoverage, SessionMessageTimelineCoverage, StepOutput, ThreadMessageReplayStats,
    ThreadMessageTimelineEntry, ThreadReplayProjection, ThreadReplayReport,
    ThreadTurnReplaySummary, TurnKernelProjection, TurnMachine, TurnReplayReport,
    build_session_message_coverage, build_session_message_timeline_coverage,
    build_thread_replay_report, compare_projection_to_live, emit_kernel, emit_kernel_event,
    events_for_step, kernel_resume_hints_from_projection,
    kernel_resume_hints_from_thread_projection, outcome_from_status, plan_v3_step_effects,
    replay_effect_counts, replay_step_effects, replay_thread_message_stats,
    replay_thread_message_timeline, replay_thread_projection, replay_turn_effects,
    replay_turn_projection, verify_effect_replay_chain, verify_guard_projection_chain,
    verify_memory_projection_chain, verify_message_timeline_coherence,
    verify_message_timeline_vs_session, verify_session_message_coverage, verify_step_effect_parity,
    verify_timeline_vs_request_count, verify_turn_replay_coherence,
};
pub use turn_port::TurnEnginePort;
