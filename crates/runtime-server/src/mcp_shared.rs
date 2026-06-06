//! Process-wide shared [`McpPool`] for the HTTP sidecar.
//!
//! All runtime engines reuse this pool (via [`crate::core::engine::tool_context`])
//! so MCP config can be hot-reloaded without restarting the sidecar.

use std::sync::{Arc, OnceLock};

use tokio::sync::Mutex as AsyncMutex;

use crate::mcp::McpPool;

static SHARED_MCP_POOL: OnceLock<Arc<AsyncMutex<McpPool>>> = OnceLock::new();

/// Install the sidecar's shared MCP pool (call once at HTTP server startup).
pub fn install_shared_mcp_pool(pool: Arc<AsyncMutex<McpPool>>) {
    let _ = SHARED_MCP_POOL.set(pool);
}

/// Clone the shared pool handle, if installed.
#[must_use]
pub fn shared_mcp_pool() -> Option<Arc<AsyncMutex<McpPool>>> {
    SHARED_MCP_POOL.get().cloned()
}
