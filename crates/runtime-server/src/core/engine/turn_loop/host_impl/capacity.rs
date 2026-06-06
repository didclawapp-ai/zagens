//! Capacity checkpoint helpers for `TurnLoopHost` (P2 PR6d — `TurnLoopMode` end-to-end).

use std::sync::Arc;

use deepseek_core::chat::LlmClient;
use deepseek_core::turn::{TurnContext, TurnLoopMode};
use tokio::sync::{Mutex as AsyncMutex, RwLock};

use super::super::Engine;
use crate::mcp::McpPool;
use crate::tools::ToolRegistry;

impl Engine {
    pub(super) async fn turn_loop_capacity_pre_request(
        &mut self,
        turn: &TurnContext,
        client: Option<&dyn LlmClient>,
        mode: TurnLoopMode,
    ) -> bool {
        Self::run_capacity_pre_request_checkpoint(self, turn, client, mode).await
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn turn_loop_capacity_post_tool(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        tool_registry: Option<&ToolRegistry>,
        tool_exec_lock: Arc<RwLock<()>>,
        mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
        step_error_count: usize,
        consecutive_tool_error_steps: u32,
    ) -> bool {
        Self::run_capacity_post_tool_checkpoint(
            self,
            turn,
            mode,
            tool_registry,
            tool_exec_lock,
            mcp_pool,
            step_error_count,
            consecutive_tool_error_steps,
        )
        .await
    }

    pub(super) async fn turn_loop_capacity_error_escalation(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        step_error_count: usize,
        consecutive_tool_error_steps: u32,
        error_categories: &[deepseek_core::error_taxonomy::ErrorCategory],
    ) -> bool {
        Self::run_capacity_error_escalation_checkpoint(
            self,
            turn,
            mode,
            step_error_count,
            consecutive_tool_error_steps,
            error_categories,
        )
        .await
    }
}
