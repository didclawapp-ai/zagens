//! MCP server list (read-only config).

use crate::cli::mcp_config::load_mcp_config;
use crate::config::Config;

pub fn list_servers(config: &Config) -> Vec<String> {
    let path = config.mcp_config_path();
    let Ok(mcp) = load_mcp_config(&path) else {
        return vec![format!("MCP config: {}", path.display())];
    };
    if mcp.servers.is_empty() {
        return vec![
            format!("MCP: {}", path.display()),
            "(no servers configured)".to_string(),
        ];
    }
    let mut lines = vec![format!("MCP: {}", path.display())];
    for (name, server) in &mcp.servers {
        let transport = if server.command.is_some() {
            "stdio"
        } else if server.url.is_some() {
            "http"
        } else {
            "?"
        };
        let tool_hint = server.enabled_tools.len();
        lines.push(format!("  {name} ({transport}, tools:{tool_hint})"));
    }
    lines
}
