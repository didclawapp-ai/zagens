//! MCP tool execution via `McpPool`.

use std::sync::Arc;

use super::super::*;

impl Engine {
    pub(in crate::core::engine) async fn execute_mcp_tool_with_pool(
        pool: Arc<AsyncMutex<McpPool>>,
        name: &str,
        input: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let mut pool = pool.lock().await;
        let result = pool
            .call_tool(name, input)
            .await
            .map_err(|e| ToolError::execution_failed(format!("MCP tool failed: {e}")))?;
        // Extract the spec content blocks instead of dumping the raw JSON
        // envelope (content/isError/meta) at the model, and honor the
        // tool-level `isError` flag so failures aren't reported as success.
        let content = crate::mcp::extract_tool_content(&result);
        if crate::mcp::is_tool_error(&result) {
            Ok(ToolResult::error(content))
        } else {
            Ok(ToolResult::success(content))
        }
    }
}
