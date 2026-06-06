//! Async MCP (Model Context Protocol) implementation.

#![allow(unused_imports, reason = "tests.inc uses `super::*` for config_io helpers")]

mod auth;
mod observability;
mod config;
mod config_io;
mod connection;
mod diagnostics;
mod format;
mod pool;
mod stdio_spawn;
mod transport;
mod types;

pub use auth::{merge_preserved_secrets, McpAuthConfig};
pub use observability::{record_mcp_call, recent_mcp_calls, McpCallRecord};
pub use config::{McpConfig, McpServerConfig, McpTimeouts, McpTransportKind};
pub use config_io::{
    add_server_config, discover_manager_snapshot, get_server_entry, init_config, load_config,
    manager_snapshot_from_config, manager_snapshot_from_pool, merge_mcp_json_fragment,
    remove_server_config,
    remove_server_from_config, replace_server_in_config, save_config, set_server_enabled,
    McpDiscoveredItem, McpManagerSnapshot, McpServerSnapshot, McpWriteStatus,
};
pub use connection::McpConnection;
pub use format::{extract_tool_content, format_tool_result, is_tool_error};
pub use pool::{McpPool, McpReloadReport};
pub use transport::McpTransport;
pub use types::{
    ConnectionState, McpPrompt, McpPromptArgument, McpResource, McpResourceTemplate, McpTool,
};

use deepseek_core::engine::hosts::McpHost;

impl McpHost for McpPool {}

#[cfg(test)]
include!("tests.inc.rs");
