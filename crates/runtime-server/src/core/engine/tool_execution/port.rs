//! Core turn-loop ports implemented by the TUI `Engine` L2.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use serde_json::json;
use tokio::sync::Mutex as AsyncMutex;
use zagens_core::engine::turn_loop::{McpPoolPort, TurnLoopToolExec, TurnLoopToolExecutor};
use zagens_tools::{ToolError, ToolResult};

use crate::mcp::McpPool;
use crate::tools::ToolRegistry;
use crate::tools::spec::ToolContext;
use zagens_core::engine::emit_tool_audit;

use super::super::Engine;

/// `McpPool` behind `Arc<Mutex<_>>` for [`McpPoolPort`].
pub struct McpPoolHandle(pub Arc<AsyncMutex<McpPool>>);

#[async_trait]
impl McpPoolPort for McpPoolHandle {
    async fn execute_tool(&self, tool_name: &str, input: Value) -> Result<ToolResult, ToolError> {
        Engine::execute_mcp_tool_with_pool(self.0.clone(), tool_name, input).await
    }
}

#[async_trait]
impl TurnLoopToolExecutor for Engine {
    type ToolRegistry = ToolRegistry;

    async fn execute_with_lock(
        &self,
        exec: TurnLoopToolExec,
        supports_parallel: bool,
        interactive: bool,
        tool_name: String,
        tool_input: Value,
        registry: Option<&Self::ToolRegistry>,
        mcp_pool: Option<Arc<AsyncMutex<dyn McpPoolPort + Send + Sync>>>,
        tool_progress_id: Option<String>,
    ) -> Result<ToolResult, ToolError> {
        if McpPool::is_mcp_tool(&tool_name) {
            let Some(pool) = mcp_pool else {
                return Err(ToolError::not_available(format!(
                    "tool '{tool_name}' is not registered"
                )));
            };
            let guard = pool.lock().await;
            return guard.execute_tool(&tool_name, tool_input).await;
        }

        Engine::execute_tool_with_lock(
            exec.lock,
            supports_parallel,
            interactive,
            exec.tx_event,
            tool_name,
            tool_input,
            registry,
            None,
            None,
            tool_progress_id,
            None,
        )
        .await
    }
}

/// Wrap the TUI MCP pool for [`TurnLoopToolExecutor::execute_with_lock`].
#[must_use]
pub fn mcp_pool_as_port(
    pool: Arc<AsyncMutex<McpPool>>,
) -> Arc<AsyncMutex<dyn McpPoolPort + Send + Sync>> {
    Arc::new(AsyncMutex::new(McpPoolHandle(pool))) as Arc<AsyncMutex<dyn McpPoolPort + Send + Sync>>
}

/// Execute a tool plan without `&mut Engine` (parallel `FuturesUnordered` tasks).
#[allow(clippy::too_many_arguments)]
pub async fn detached_execute_with_lock(
    exec: TurnLoopToolExec,
    supports_parallel: bool,
    interactive: bool,
    tool_name: String,
    tool_input: Value,
    registry: Option<&ToolRegistry>,
    mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
    tool_progress_id: Option<String>,
    lock_ctx: Option<crate::tools::resource_locks::FineGrainedLockContext>,
) -> Result<ToolResult, ToolError> {
    if McpPool::is_mcp_tool(&tool_name) {
        let Some(pool) = mcp_pool else {
            return Err(ToolError::not_available(format!(
                "tool '{tool_name}' is not registered"
            )));
        };
        return Engine::execute_mcp_tool_with_pool(pool, &tool_name, tool_input).await;
    }

    Engine::execute_tool_with_lock(
        exec.lock,
        supports_parallel,
        interactive,
        exec.tx_event,
        tool_name,
        tool_input,
        registry,
        mcp_pool,
        None,
        tool_progress_id,
        lock_ctx,
    )
    .await
}

/// Sequential plan execution with optional elevated [`ToolContext`] (approval path).
#[allow(clippy::too_many_arguments)]
pub async fn execute_plan_on_engine(
    exec: TurnLoopToolExec,
    supports_parallel: bool,
    interactive: bool,
    tool_name: String,
    tool_input: Value,
    registry: Option<&ToolRegistry>,
    mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
    context_override: Option<ToolContext>,
    tool_progress_id: Option<String>,
    lock_ctx: Option<crate::tools::resource_locks::FineGrainedLockContext>,
) -> Result<ToolResult, ToolError> {
    if McpPool::is_mcp_tool(&tool_name) {
        let Some(pool) = mcp_pool else {
            return Err(ToolError::not_available(format!(
                "tool '{tool_name}' is not registered"
            )));
        };
        return Engine::execute_mcp_tool_with_pool(pool, &tool_name, tool_input).await;
    }

    Engine::execute_tool_with_lock(
        exec.lock,
        supports_parallel,
        interactive,
        exec.tx_event,
        tool_name,
        tool_input,
        registry,
        mcp_pool,
        context_override,
        tool_progress_id,
        lock_ctx,
    )
    .await
}

/// Apply spillover + audit when a tool result exceeds inline limits.
pub fn apply_tool_spillover_audit(tool_result: &mut ToolResult, tool_id: &str, tool_name: &str) {
    if let Some(path) = crate::tools::truncate::apply_spillover(tool_result, tool_id) {
        emit_tool_audit(json!({
            "event": "tool.spillover",
            "tool_id": tool_id,
            "tool_name": tool_name,
            "path": path.display().to_string(),
        }));
    }
}
