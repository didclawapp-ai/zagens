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

use super::control::{TurnLoopControl, TurnLoopStreamingPhaseOutcome, TurnLoopToolPhaseOutcome};
use crate::engine::loop_guard::LoopGuard;
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

/// Opaque MCP pool type (TUI: `crate::mcp::McpPool`).
pub trait TurnLoopMcpPool: Send + Sync {}

#[async_trait]
pub trait TurnLoopHost: Send {
    type ToolRegistry: TurnLoopToolRegistry;
    type McpPool: TurnLoopMcpPool;

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

    async fn run_streaming_phase(
        &mut self,
        turn: &mut TurnContext,
        client: &dyn LlmClient,
        mode: TurnLoopMode,
        tool_catalog: &[Tool],
        active_tool_names: &HashSet<String>,
        force_update_plan_first: bool,
        stream_retry_attempts: &mut u32,
        context_recovery_attempts: &mut u8,
        turn_error: &mut Option<String>,
    ) -> TurnLoopStreamingPhaseOutcome;

    async fn run_tool_execution_phase(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        tool_uses: &mut [ToolUseState],
        tool_catalog: &[Tool],
        active_tool_names: &mut HashSet<String>,
        loop_guard: &mut LoopGuard,
        consecutive_tool_error_steps: u32,
        tool_registry: Option<&Self::ToolRegistry>,
    ) -> TurnLoopToolPhaseOutcome;

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
