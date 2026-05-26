use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::json;
use tokio::process::Command;

use crate::network_policy::{Decision, NetworkPolicyDecider, host_from_url};

use super::config::{McpServerConfig, McpTimeouts};
use super::transport::{McpTransport, SseTransport, StdioTransport};
use super::types::{
    ConnectionState, McpPrompt, McpResource, McpResourceTemplate, McpTool,
};

pub struct McpConnection {
    name: String,
    pub(super) transport: Box<dyn McpTransport>,
    tools: Vec<McpTool>,
    resources: Vec<McpResource>,
    resource_templates: Vec<McpResourceTemplate>,
    prompts: Vec<McpPrompt>,
    request_id: AtomicU64,
    state: ConnectionState,
    config: McpServerConfig,
    cancel_token: tokio_util::sync::CancellationToken,
}

impl McpConnection {
    /// Connect to an MCP server and initialize it.
    ///
    /// `network_policy` (added in v0.7.0 for #135) is consulted for HTTP/SSE
    /// transports only — STDIO transports are unaffected. Pass `None` to
    /// match pre-v0.7.0 permissive behavior.
    pub async fn connect_with_policy(
        name: String,
        config: McpServerConfig,
        global_timeouts: &McpTimeouts,
        network_policy: Option<&NetworkPolicyDecider>,
    ) -> Result<Self> {
        let connect_timeout_secs = config.effective_connect_timeout(global_timeouts);
        let cancel_token = tokio_util::sync::CancellationToken::new();

        let transport: Box<dyn McpTransport> = if let Some(url) = &config.url {
            // Per-domain network policy gate (#135). Only the HTTP/SSE transport
            // is gated; STDIO MCP servers run as local subprocesses and never
            // touch the network from this code path.
            if let Some(decider) = network_policy
                && let Some(host) = host_from_url(url)
            {
                match decider.evaluate(&host, "mcp") {
                    Decision::Allow => {}
                    Decision::Deny => {
                        anyhow::bail!(
                            "MCP server '{name}' connection to '{host}' blocked by network policy"
                        );
                    }
                    Decision::Prompt => {
                        anyhow::bail!(
                            "MCP server '{name}' connection to '{host}' requires approval; \
                             re-run after `/network allow {host}` or set network.default = \"allow\" in config"
                        );
                    }
                }
            }
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(connect_timeout_secs))
                .build()?;
            Box::new(SseTransport::connect(client, url.clone(), cancel_token.clone()).await?)
        } else if let Some(command) = &config.command {
            let mut cmd = tokio::process::Command::new(command);
            cmd.args(&config.args)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true);

            for (key, value) in &config.env {
                cmd.env(key, value);
            }

            let mut child = cmd.spawn().with_context(|| {
                let env_keys: Vec<&str> = config.env.keys().map(String::as_str).collect();
                format!(
                    "MCP stdio spawn failed (transport=stdio server={name} cmd={command:?} args={:?} env_keys={env_keys:?})",
                    config.args,
                )
            })?;

            let stdin = child.stdin.take().context("Failed to get MCP stdin")?;
            let stdout = child.stdout.take().context("Failed to get MCP stdout")?;

            Box::new(StdioTransport {
                child,
                stdin,
                reader: tokio::io::BufReader::new(stdout),
            })
        } else {
            anyhow::bail!(
                "MCP server '{}' config must have either 'command' or 'url'",
                name
            );
        };

        let mut conn = Self {
            name: name.clone(),
            transport,
            tools: Vec::new(),
            resources: Vec::new(),
            resource_templates: Vec::new(),
            prompts: Vec::new(),
            request_id: AtomicU64::new(1),
            state: ConnectionState::Connecting,
            config,
            cancel_token,
        };

        // Initialize with timeout
        tokio::time::timeout(Duration::from_secs(connect_timeout_secs), conn.initialize())
            .await
            .with_context(|| format!("MCP server '{name}' initialization timed out"))??;

        // Discover tools, resources, and prompts with timeout
        tokio::time::timeout(
            Duration::from_secs(connect_timeout_secs),
            conn.discover_all(),
        )
        .await
        .with_context(|| format!("MCP server '{name}' discovery timed out"))??;

        conn.state = ConnectionState::Ready;
        Ok(conn)
    }

    /// Send initialize request and wait for response
    async fn initialize(&mut self) -> Result<()> {
        let init_id = self.next_id();
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "id": init_id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "clientInfo": {
                    "name": "deepseek-tui",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "tools": {},
                    "resources": {},
                    "prompts": {}
                }
            }
        }))
        .await?;

        self.recv(init_id).await?;

        // Send initialized notification (no id, no response expected)
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
        .await?;

        Ok(())
    }

    /// Discover tools, resources, and prompts
    async fn discover_all(&mut self) -> Result<()> {
        // We use join! to discover everything concurrently if possible,
        // but for now let's keep it sequential for simplicity in error handling
        self.discover_tools().await?;
        self.discover_resources().await?;
        self.discover_resource_templates().await?;
        self.discover_prompts().await?;
        Ok(())
    }

    /// Discover available tools from the MCP server
    async fn discover_tools(&mut self) -> Result<()> {
        let list_id = self.next_id();
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "id": list_id,
            "method": "tools/list",
            "params": {}
        }))
        .await?;

        let response = self.recv(list_id).await?;

        if let Some(result) = response.get("result")
            && let Some(tools) = result.get("tools")
        {
            self.tools = serde_json::from_value(tools.clone()).unwrap_or_default();
        }

        Ok(())
    }

    /// Discover available resources from the MCP server
    async fn discover_resources(&mut self) -> Result<()> {
        let list_id = self.next_id();
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "id": list_id,
            "method": "resources/list",
            "params": {}
        }))
        .await?;

        let response = self.recv(list_id).await?;

        if let Some(result) = response.get("result")
            && let Some(resources) = result.get("resources")
        {
            self.resources = serde_json::from_value(resources.clone()).unwrap_or_default();
        }

        Ok(())
    }

    /// Discover available resource templates from the MCP server
    async fn discover_resource_templates(&mut self) -> Result<()> {
        let list_id = self.next_id();
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "id": list_id,
            "method": "resources/templates/list",
            "params": {}
        }))
        .await?;

        let response = self.recv(list_id).await?;

        if let Some(result) = response.get("result") {
            let templates = result
                .get("resourceTemplates")
                .or_else(|| result.get("templates"))
                .or_else(|| result.get("resource_templates"));
            if let Some(templates) = templates {
                self.resource_templates =
                    serde_json::from_value(templates.clone()).unwrap_or_default();
            }
        }

        Ok(())
    }

    /// Discover available prompts from the MCP server
    async fn discover_prompts(&mut self) -> Result<()> {
        let list_id = self.next_id();
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "id": list_id,
            "method": "prompts/list",
            "params": {}
        }))
        .await?;

        let response = self.recv(list_id).await?;

        if let Some(result) = response.get("result")
            && let Some(prompts) = result.get("prompts")
        {
            self.prompts = serde_json::from_value(prompts.clone()).unwrap_or_default();
        }

        Ok(())
    }

    /// Call a tool on this MCP server
    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: serde_json::Value,
        timeout_secs: u64,
    ) -> Result<serde_json::Value> {
        self.call_method(
            "tools/call",
            serde_json::json!({
                "name": tool_name,
                "arguments": arguments
            }),
            timeout_secs,
        )
        .await
    }

    /// Read a resource from this MCP server
    pub async fn read_resource(
        &mut self,
        uri: &str,
        timeout_secs: u64,
    ) -> Result<serde_json::Value> {
        self.call_method(
            "resources/read",
            serde_json::json!({
                "uri": uri
            }),
            timeout_secs,
        )
        .await
    }

    /// Get a prompt from this MCP server
    pub async fn get_prompt(
        &mut self,
        prompt_name: &str,
        arguments: serde_json::Value,
        timeout_secs: u64,
    ) -> Result<serde_json::Value> {
        self.call_method(
            "prompts/get",
            serde_json::json!({
                "name": prompt_name,
                "arguments": arguments
            }),
            timeout_secs,
        )
        .await
    }

    /// Generic method to call an MCP method
    async fn call_method(
        &mut self,
        method: &str,
        params: serde_json::Value,
        timeout_secs: u64,
    ) -> Result<serde_json::Value> {
        if self.state != ConnectionState::Ready {
            anyhow::bail!(
                "Failed to call MCP method '{}': connection '{}' is not ready",
                method,
                self.name
            );
        }

        let call_id = self.next_id();
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "id": call_id,
            "method": method,
            "params": params
        }))
        .await?;

        let response = tokio::time::timeout(Duration::from_secs(timeout_secs), self.recv(call_id))
            .await
            .with_context(|| {
                format!(
                    "MCP method '{}' on server '{}' timed out after {}s",
                    method, self.name, timeout_secs
                )
            })??;

        if let Some(error) = response.get("error") {
            return Err(anyhow::anyhow!(
                "MCP error in '{}': {}",
                method,
                serde_json::to_string_pretty(error)?
            ));
        }

        Ok(response
            .get("result")
            .cloned()
            .unwrap_or(serde_json::json!(null)))
    }

    /// Get discovered tools
    pub fn tools(&self) -> &[McpTool] {
        &self.tools
    }

    /// Get discovered resources
    pub fn resources(&self) -> &[McpResource] {
        &self.resources
    }

    /// Get discovered resource templates
    pub fn resource_templates(&self) -> &[McpResourceTemplate] {
        &self.resource_templates
    }

    /// Get discovered prompts
    pub fn prompts(&self) -> &[McpPrompt] {
        &self.prompts
    }

    /// Get server name
    #[allow(dead_code)] // Public API for MCP consumers
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Check if connection is ready
    pub fn is_ready(&self) -> bool {
        self.state == ConnectionState::Ready
    }

    /// Get server config
    pub fn config(&self) -> &McpServerConfig {
        &self.config
    }

    /// Get connection state
    #[allow(dead_code)] // Public API for MCP consumers
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    async fn send(&mut self, msg: serde_json::Value) -> Result<()> {
        self.transport.send(msg).await
    }

    async fn recv(&mut self, expected_id: u64) -> Result<serde_json::Value> {
        loop {
            let value = self.transport.recv().await.inspect_err(|_e| {
                self.state = ConnectionState::Disconnected;
            })?;

            // Check if this is a response with the expected id
            if value.get("id").and_then(serde_json::Value::as_u64) == Some(expected_id) {
                return Ok(value);
            }
            // Skip notifications (no id) and responses with different ids
        }
    }

    /// Gracefully close the connection
    #[allow(dead_code)] // Public API for MCP consumers
    pub fn close(&mut self) {
        self.cancel_token.cancel();
        self.state = ConnectionState::Disconnected;
    }
}

impl Drop for McpConnection {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}
