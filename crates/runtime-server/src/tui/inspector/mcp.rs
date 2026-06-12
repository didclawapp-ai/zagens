//! MCP server list (read-only config) with expandable tool names.

use crate::cli::mcp_config::load_mcp_config;
use crate::config::Config;

#[derive(Debug, Clone)]
pub struct McpServerEntry {
    pub name: String,
    pub transport: &'static str,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct McpPanelState {
    pub header: String,
    pub servers: Vec<McpServerEntry>,
}

impl Default for McpPanelState {
    fn default() -> Self {
        Self {
            header: String::new(),
            servers: Vec::new(),
        }
    }
}

impl McpPanelState {
    pub fn line_count(&self, expanded: Option<&str>) -> usize {
        let mut n = self.servers.len().max(1);
        if let Some(name) = expanded {
            if let Some(server) = self.servers.iter().find(|s| s.name == name) {
                n += server.tools.len().max(1);
            }
        }
        n
    }
}

pub fn load_mcp_panel(config: &Config) -> McpPanelState {
    let path = config.mcp_config_path();
    let Ok(mcp) = load_mcp_config(&path) else {
        return McpPanelState {
            header: format!("MCP: {}", path.display()),
            servers: Vec::new(),
        };
    };
    if mcp.servers.is_empty() {
        return McpPanelState {
            header: format!("MCP: {}", path.display()),
            servers: Vec::new(),
        };
    }
    let servers = mcp
        .servers
        .iter()
        .map(|(name, server)| {
            let transport = if server.command.is_some() {
                "stdio"
            } else if server.url.is_some() {
                "http"
            } else {
                "?"
            };
            McpServerEntry {
                name: name.clone(),
                transport,
                tools: server.enabled_tools.clone(),
            }
        })
        .collect();
    McpPanelState {
        header: format!("MCP: {}", path.display()),
        servers,
    }
}

pub fn list_servers(config: &Config) -> Vec<String> {
    let panel = load_mcp_panel(config);
    let mut lines = vec![panel.header.clone()];
    if panel.servers.is_empty() {
        lines.push("(no servers configured)".to_string());
        return lines;
    }
    for server in &panel.servers {
        lines.push(format!(
            "  {} ({}, tools:{})",
            server.name,
            server.transport,
            server.tools.len()
        ));
    }
    lines
}
