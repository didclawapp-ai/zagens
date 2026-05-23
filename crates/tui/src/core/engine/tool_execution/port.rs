//! Core turn-loop ports implemented by the TUI `Engine` L2.

use std::sync::Arc;

use async_trait::async_trait;
use deepseek_core::engine::turn_loop::{
    McpPoolPort, TurnLoopToolExec, TurnLoopToolExecutor, TurnLoopToolRegistry,
};
use deepseek_tools::{ToolError, ToolResult};
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

use crate::mcp::McpPool;
use crate::tools::ToolRegistry;

use super::super::Engine;

/// `McpPool` behind `Arc<Mutex<_>>` for [`McpPoolPort`].
pub struct McpPoolHandle(pub Arc<AsyncMutex<McpPool>>);

#[async_trait]
impl McpPoolPort for McpPoolHandle {
    async fn execute_tool(
        &self,
        tool_name: &str,
        input: Value,
    ) -> Result<ToolResult, ToolError> {
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
        )
        .await
    }
}
