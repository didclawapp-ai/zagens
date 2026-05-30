//! `TurnLoopHost` port for migrating `handle_deepseek_turn` into `deepseek-core` (P2 PR4).

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use deepseek_tools::{ToolError, ToolResult};
use serde_json::Value;

use tokio::sync::{mpsc, Mutex as AsyncMutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::chat::{LlmClient, Message, Tool};
use crate::compaction::CompactionConfig;
use crate::error_taxonomy::ErrorCategory;
use crate::events::Event;
use crate::scratchpad::ScratchpadConfig;
use crate::session::Session;
use crate::turn::{TurnContext, TurnLoopMode};

use super::control::TurnLoopControl;
use super::exec::{ToolExecOutcome, ToolExecutionPlan, ToolPlanApprovalMeta};
use crate::engine::streaming::ToolUseState;

/// Config slices the turn loop reads each step (avoids pulling full `EngineConfig` into core yet).
#[derive(Debug, Clone, Copy)]
pub struct TurnLoopConfigView<'a> {
    pub compaction: &'a CompactionConfig,
    pub strict_tool_mode: bool,
    pub scratchpad: &'a ScratchpadConfig,
    pub workspace: &'a Path,
}

/// Host port for `handle_deepseek_turn` (P2 PR4 step 2).
/// Opaque tool-registry type (TUI: `crate::tools::ToolRegistry`).
pub trait TurnLoopToolRegistry: Send + Sync {}

/// Deprecated alias for [`McpHost`](crate::engine::hosts::McpHost).
///
/// M4 (Engine-struct strangler) renamed this empty marker into the
/// named [`McpHost`] trait with default-impl predicate / metadata
/// methods. The blanket impl below keeps existing `impl
/// TurnLoopMcpPool for McpPool` consumers compiling for one release;
/// new code should impl `McpHost` directly. The alias and blanket
/// impl will be removed in the next release.
#[deprecated(
    since = "0.8.16",
    note = "use `deepseek_core::engine::hosts::McpHost` instead; \
            this alias will be removed in the next release"
)]
pub trait TurnLoopMcpPool: Send + Sync {}

#[allow(deprecated)]
impl<T: crate::engine::hosts::McpHost + ?Sized> TurnLoopMcpPool for T {}

#[async_trait]
pub trait TurnLoopHost: Send {
    type ToolRegistry: TurnLoopToolRegistry;
    type McpPool: crate::engine::hosts::McpHost;

    // ── Session / config accessors (disjoint borrows) ─────────────────

    fn session_mut(&mut self) -> &mut Session;

    fn compaction_config(&self) -> &CompactionConfig;

    #[must_use]
    fn compaction_enabled(&self) -> bool {
        self.compaction_config().enabled
    }

    fn workspace(&self) -> &Path;

    #[must_use]
    fn strict_tool_mode(&self) -> bool;

    fn scratchpad_config(&self) -> &ScratchpadConfig;

    fn scratchpad_run_id(&self) -> Option<&str>;

    fn scratchpad_summary_injected_mut(&mut self) -> &mut bool;

    fn cancel_token(&self) -> &CancellationToken;

    fn tx_event(&self) -> &mpsc::Sender<Event>;

    fn rx_steer_mut(&mut self) -> &mut mpsc::Receiver<String>;

    fn tool_exec_lock(&self) -> Arc<RwLock<()>>;

    fn llm_client(&self) -> Option<Arc<dyn LlmClient>>;

    // ── Tool catalog helpers (L2: deferral / search / code-exec) ───────

    fn prepare_tool_catalog(&self, catalog: &mut Vec<Tool>);

    fn initial_active_tool_names(&self, catalog: &[Tool]) -> HashSet<String>;

    fn active_tools_for_step(
        &self,
        catalog: &[Tool],
        active: &HashSet<String>,
        force_update_plan_first: bool,
    ) -> Vec<Tool>;

    fn is_mcp_tool_name(&self, name: &str) -> bool;

    fn maybe_activate_deferred_tool(
        &self,
        tool_name: &str,
        catalog: &[Tool],
        active: &mut HashSet<String>,
    ) -> bool;

    async fn execute_code_execution_tool(
        &self,
        input: &Value,
        workspace: &Path,
    ) -> Result<ToolResult, ToolError>;

    fn execute_tool_search(
        &self,
        tool_name: &str,
        input: &Value,
        catalog: &[Tool],
        active: &mut HashSet<String>,
    ) -> Result<ToolResult, ToolError>;

    // ── Lifecycle hooks ───────────────────────────────────────────────

    fn reset_scratchpad_step(&mut self);

    async fn refresh_system_prompt(&mut self, mode: TurnLoopMode);

    async fn add_session_message(&mut self, message: Message);

    async fn emit_session_updated(&mut self);

    async fn run_auto_compaction(&mut self, client: &dyn LlmClient);

    fn estimated_input_tokens(&self) -> usize;

    async fn flush_pending_lsp_diagnostics(&mut self);

    async fn layered_context_checkpoint(&mut self);

    fn decorate_auth_error_message(&self, message: String) -> String;

    async fn recover_context_overflow(
        &mut self,
        client: &dyn LlmClient,
        reason: &str,
        max_output_tokens: u32,
    ) -> bool;

    async fn run_capacity_pre_request_checkpoint(
        &mut self,
        turn: &TurnContext,
        client: Option<&dyn LlmClient>,
        mode: TurnLoopMode,
    ) -> bool;

    async fn run_capacity_post_tool_checkpoint(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        tool_registry: Option<&Self::ToolRegistry>,
        tool_exec_lock: Arc<RwLock<()>>,
        mcp_pool: Option<Arc<AsyncMutex<Self::McpPool>>>,
        step_error_count: usize,
        consecutive_tool_error_steps: u32,
    ) -> bool;

    /// L2: resolve `auto` reasoning_effort (TUI: `auto_reasoning`; core default: session only).
    fn effective_reasoning_effort_for_request(&mut self) -> Option<String>;

    /// L2: streaming SSE tool JSON parse (TUI: `arg_repair` ladder).
    fn parse_streaming_tool_input(&self, buffer: &str) -> Option<Value>;

    /// L2: finalized tool input after stream block stop.
    fn final_streaming_tool_input(&self, state: &ToolUseState) -> Value;

    /// L2: ensure MCP pool when the batch includes MCP tool names.
    async fn ensure_mcp_pool_for_tools(
        &mut self,
        tool_uses: &[ToolUseState],
    ) -> Option<Arc<AsyncMutex<Self::McpPool>>>;

    /// L2: resolve a hallucinated tool name via registry alias table.
    fn resolve_hallucinated_tool_name(
        &self,
        name: &str,
        catalog: &[Tool],
        registry: Option<&Self::ToolRegistry>,
    ) -> Option<String>;

    /// L2: approval / parallelism metadata for one planned tool.
    fn tool_plan_approval_meta(
        &self,
        tool_name: &str,
        tool_input: &Value,
        registry: Option<&Self::ToolRegistry>,
    ) -> ToolPlanApprovalMeta;

    /// L2: run parallel/sequential execution for planned tools (TUI: `tool_plans_exec`).
    async fn execute_tool_plans(
        &mut self,
        mode: TurnLoopMode,
        plans: Vec<ToolExecutionPlan>,
        tool_catalog: &[Tool],
        active_tool_names: &mut HashSet<String>,
        tool_registry: Option<&Self::ToolRegistry>,
        mcp_pool: Option<Arc<AsyncMutex<Self::McpPool>>>,
        tool_exec_lock: Arc<RwLock<()>>,
    ) -> Vec<ToolExecOutcome>;

    async fn run_capacity_error_escalation_checkpoint(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        step_error_count: usize,
        consecutive_tool_error_steps: u32,
        error_categories: &[ErrorCategory],
    ) -> bool;

    async fn run_post_edit_lsp_hook(&mut self, tool_name: &str, tool_input: &Value);

    fn record_scratchpad_tool_outcome(&mut self, tool_name: &str, success: bool);

    async fn record_long_horizon_tool_outcome(
        &mut self,
        _tool_name: &str,
        _tool_input: &Value,
        _result: &str,
        _success: bool,
    ) {
    }

    /// Suffix to append to tool result text (verify mismatch, etc.).
    fn take_long_horizon_tool_suffix(&mut self) -> Option<String> {
        None
    }

    /// Emit warning-band status once; reinject objective on configured step interval.
    async fn maybe_lht_pre_request_hooks(&mut self, _mode: TurnLoopMode) {}

    /// At the `max_steps` cap: give a long-horizon host the chance to convert a
    /// step-exhaustion stop into a bounded continuation. Returns `true` if the
    /// host injected a continue nudge and the loop should be granted another
    /// step-budget window (caller bounds the number of grants); `false` keeps
    /// the original "Reached maximum steps" termination. Default: no
    /// continuation (non-LHT hosts terminate at the cap as before).
    async fn maybe_continue_at_step_limit(&mut self, _turn: &TurnContext) -> bool {
        false
    }

    /// On a `LoopGuard` halt (a tool failed too many times in a row): give a
    /// long-horizon host one bounded chance to convert the stuck-stop into a
    /// "change approach" continuation. Returns `true` if the host injected a
    /// nudge and the loop should keep going (the caller resets the guard's
    /// failure counters and bounds the number of grants); `false` keeps the
    /// original halt → turn termination. Default: no continuation.
    async fn maybe_continue_after_loop_guard_halt(&mut self, _turn: &TurnContext) -> bool {
        false
    }

    /// When an in-flight turn's context overflows the model budget and
    /// emergency compaction has been exhausted (the loop is about to hard-fail
    /// the turn and tell the user to run `/compact`): give a long-horizon host
    /// one bounded chance to roll a **cycle handoff** instead. A handoff
    /// summarizes the conversation into a small `<carry_forward>` briefing seed
    /// and preserves structured task state (plan / todos / working set /
    /// handoff.md), so the next step continues in the same thread with a fresh,
    /// in-budget context. Returns `true` if the host rotated the cycle (the
    /// caller resets its recovery budget, bounds the number of handoffs, and
    /// retries the request); `false` keeps the original hard failure. Default:
    /// no handoff (non-LHT / cycle-disabled hosts fail as before).
    async fn maybe_cycle_handoff_on_context_overflow(
        &mut self,
        _turn: &TurnContext,
        _mode: TurnLoopMode,
    ) -> bool {
        false
    }

    /// At a per-step safe boundary inside the turn loop (a tool step completed,
    /// no in-flight stream / pending approval): give a long-horizon host the
    /// chance to evaluate the **clean** cycle-advance gate (context threshold /
    /// long-horizon early-advance band) and roll a cycle handoff if crossed.
    /// The gate is otherwise only checked *between turns*, so a long-horizon
    /// turn that loops many tool steps without returning would only ever get
    /// the hard-overflow emergency handoff, never a clean early refresh (#5).
    /// Returns `true` when a handoff happened (the caller bounds the count and
    /// re-loops with the fresh, in-budget context); `false` otherwise. Default:
    /// no-op (non-LHT / cycle-disabled hosts are unaffected).
    async fn maybe_advance_cycle_at_checkpoint(&mut self, _mode: TurnLoopMode) -> bool {
        false
    }

    /// Defensive observability at the final turn fallthrough. Every `break` in
    /// the outer loop converges to a `Completed` outcome; if a long-horizon
    /// task is still incomplete at that point (not cancelled, no error), the
    /// host should surface that the stop was a give-up rather than a genuine
    /// completion, so the UI / logs don't show a false green. Default: no-op.
    async fn note_incomplete_stop_if_lht(&mut self) {}

    /// Called after a successful scratchpad bind/write tool so the host can sync run_id
    /// and eager-load audit sub-agent tools in the same turn.
    fn on_audit_scratchpad_bind_success(
        &mut self,
        _mode: TurnLoopMode,
        _tool_name: &str,
        _catalog: &mut [Tool],
        _active: &mut HashSet<String>,
    ) {
    }

    async fn maybe_inject_scratchpad_summary(&mut self) -> bool;

    async fn maybe_inject_scratchpad_reminder(&mut self);

    async fn handle_no_tool_uses(
        &mut self,
        turn: &mut TurnContext,
        pending_steers: &mut Vec<String>,
        current_text_visible: &str,
        has_sendable_assistant_content: bool,
    ) -> TurnLoopControl;

    fn pre_tool_snapshot(&self, workspace: &Path, tool_id: &str);
}
