//! Tool execution port for parallel/sequential paths in the turn loop (P2 PR4).

use std::sync::Arc;

use async_trait::async_trait;
use deepseek_tools::{ToolError, ToolRegistry, ToolResult};
use serde_json::Value;
use tokio::sync::{mpsc, RwLock};

use crate::events::Event;

/// Cloned handles passed into `async move` tool tasks.
#[derive(Clone)]
pub struct TurnLoopToolExec {
    pub lock: Arc<RwLock<()>>,
    pub tx_event: mpsc::Sender<Event>,
}

/// Executes registry/MCP tools for the turn loop (implemented by TUI `Engine`).
#[async_trait]
pub trait TurnLoopToolExecutor: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    async fn execute_with_lock(
        &self,
        exec: TurnLoopToolExec,
        supports_parallel: bool,
        interactive: bool,
        tool_name: String,
        tool_input: Value,
        registry: Option<&ToolRegistry>,
        mcp_pool: Option<Arc<tokio::sync::Mutex<dyn McpPoolPort + Send + Sync>>>,
        context_override: Option<serde_json::Value>,
        tool_progress_id: Option<String>,
    ) -> Result<ToolResult, ToolError>;

}

/// Opaque MCP pool surface for the core turn loop.
#[async_trait]
pub trait McpPoolPort: Send + Sync {
    async fn execute_tool(
        &self,
        tool_name: &str,
        input: Value,
    ) -> Result<ToolResult, ToolError>;
}
