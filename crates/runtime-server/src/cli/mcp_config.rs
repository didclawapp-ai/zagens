//! MCP config load/save helpers for CLI commands.

use std::path::Path;

use anyhow::{Result, anyhow};

use crate::mcp::McpConfig;

pub(crate) fn load_mcp_config(path: &Path) -> Result<McpConfig> {
    if !path.exists() {
        return Ok(McpConfig::default());
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("Failed to read MCP config {}: {e}", path.display()))?;
    serde_json::from_str(&contents).map_err(|e| anyhow!("Failed to parse MCP config: {e}"))
}
