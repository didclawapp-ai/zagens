use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::auth::McpAuthConfig;

/// Full MCP configuration from mcp.json
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct McpConfig {
    #[serde(default)]
    pub timeouts: McpTimeouts,
    #[serde(default, alias = "mcpServers")]
    pub servers: HashMap<String, McpServerConfig>,
}

/// Global timeout configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[allow(clippy::struct_field_names)]
pub struct McpTimeouts {
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout: u64,
    #[serde(default = "default_execute_timeout")]
    pub execute_timeout: u64,
    #[serde(default = "default_read_timeout")]
    pub read_timeout: u64,
}

fn default_connect_timeout() -> u64 {
    30
}
fn default_execute_timeout() -> u64 {
    60
}
fn default_read_timeout() -> u64 {
    120
}

impl Default for McpTimeouts {
    fn default() -> Self {
        Self {
            connect_timeout: default_connect_timeout(),
            execute_timeout: default_execute_timeout(),
            read_timeout: default_read_timeout(),
        }
    }
}

/// Which wire transport a server uses. Resolved from the explicit
/// `transport`/`type` config field, falling back to inference from
/// `command` (stdio) vs `url` (SSE, for backward compatibility).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTransportKind {
    /// Local subprocess over stdin/stdout JSON-RPC lines.
    Stdio,
    /// Legacy HTTP+SSE transport (GET event stream + POST endpoint).
    Sse,
    /// 2025 Streamable HTTP transport (single endpoint POST + optional SSE).
    Http,
}

impl McpTransportKind {
    /// Stable lower-case label used in snapshots / UI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Sse => "sse",
            Self::Http => "http",
        }
    }
}

/// Configuration for a single MCP server
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct McpServerConfig {
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub url: Option<String>,
    /// Explicit transport selector (`stdio` | `sse` | `http`). When omitted,
    /// the transport is inferred: `command` ⇒ stdio, `url` ⇒ sse. Set to
    /// `http` to use the 2025 Streamable HTTP transport. Accepts the `type`
    /// alias for compatibility with common `mcp.json` snippets.
    #[serde(default, alias = "type", skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    /// Extra HTTP headers for remote transports (`sse` / `http`). Values may
    /// use `${ENV_VAR}` placeholders — prefer env indirection over plaintext
    /// secrets in `mcp.json`.
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Shorthand auth (`type`: `bearer` | `apiKey`). Merged before `headers`;
    /// explicit `headers` entries override auth defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<McpAuthConfig>,
    #[serde(default)]
    pub connect_timeout: Option<u64>,
    #[serde(default)]
    pub execute_timeout: Option<u64>,
    #[serde(default)]
    pub read_timeout: Option<u64>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub enabled_tools: Vec<String>,
    #[serde(default)]
    pub disabled_tools: Vec<String>,
}

fn default_enabled() -> bool {
    true
}

impl McpServerConfig {
    pub fn effective_connect_timeout(&self, global: &McpTimeouts) -> u64 {
        self.connect_timeout.unwrap_or(global.connect_timeout)
    }

    pub fn effective_execute_timeout(&self, global: &McpTimeouts) -> u64 {
        self.execute_timeout.unwrap_or(global.execute_timeout)
    }

    pub fn effective_read_timeout(&self, global: &McpTimeouts) -> u64 {
        self.read_timeout.unwrap_or(global.read_timeout)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && !self.disabled
    }

    /// Resolve the effective transport for this server.
    ///
    /// Honors the explicit `transport`/`type` field when present, otherwise
    /// infers from `command` (stdio) vs `url` (SSE, preserving pre-Streamable
    /// behavior). Returns an error when the config is ambiguous or incomplete.
    pub fn transport_kind(&self) -> anyhow::Result<McpTransportKind> {
        if let Some(raw) = &self.transport {
            let kind = match raw.trim().to_ascii_lowercase().as_str() {
                "stdio" => McpTransportKind::Stdio,
                "sse" => McpTransportKind::Sse,
                "http" | "streamable-http" | "streamable_http" | "streamablehttp"
                | "http-stream" => McpTransportKind::Http,
                other => anyhow::bail!(
                    "unknown MCP transport '{other}' (expected one of: stdio, sse, http)"
                ),
            };
            // Sanity-check that the required endpoint field is present.
            match kind {
                McpTransportKind::Stdio if self.command.is_none() => {
                    anyhow::bail!("transport 'stdio' requires a 'command'")
                }
                McpTransportKind::Sse | McpTransportKind::Http if self.url.is_none() => {
                    anyhow::bail!("transport '{}' requires a 'url'", kind.as_str())
                }
                _ => {}
            }
            return Ok(kind);
        }

        match (&self.command, &self.url) {
            (Some(_), _) => Ok(McpTransportKind::Stdio),
            (None, Some(_)) => Ok(McpTransportKind::Sse),
            (None, None) => {
                anyhow::bail!("MCP server config must have either 'command' or 'url'")
            }
        }
    }

    pub fn is_tool_enabled(&self, tool_name: &str) -> bool {
        let allowed = if self.enabled_tools.is_empty() {
            true
        } else {
            self.enabled_tools.iter().any(|t| t == tool_name)
        };
        if !allowed {
            return false;
        }
        !self.disabled_tools.iter().any(|t| t == tool_name)
    }
}
