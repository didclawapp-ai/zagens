use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::Instrument;

use crate::http_client::apply_env_proxy;
use crate::network_policy::{Decision, NetworkPolicyDecider, host_from_url};

use super::auth::apply_default_headers;
use super::config::{McpServerConfig, McpTimeouts, McpTransportKind};
use super::transport::{McpTransport, SseTransport, StdioTransport, StreamableHttpTransport};

/// Protocol version we advertise on `initialize` (latest spec we target).
const PREFERRED_PROTOCOL_VERSION: &str = "2025-06-18";

/// Protocol versions this client knows how to speak, newest first.
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
use super::types::{ConnectionState, McpPrompt, McpResource, McpResourceTemplate, McpTool};

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
        let read_timeout_secs = config.effective_read_timeout(global_timeouts);
        let cancel_token = tokio_util::sync::CancellationToken::new();

        let transport_kind = config
            .transport_kind()
            .with_context(|| format!("MCP server '{name}' has an invalid transport config"))?;

        let transport: Box<dyn McpTransport> = match transport_kind {
            McpTransportKind::Sse | McpTransportKind::Http => {
                let url = config
                    .url
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("MCP server '{name}' requires a 'url'"))?;

                // Per-domain network policy gate (#135). Only HTTP/SSE
                // transports are gated; STDIO MCP servers run as local
                // subprocesses and never touch the network here.
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

                let http_headers = config.resolve_http_headers(&name)?;

                if transport_kind == McpTransportKind::Http {
                    // Streamable HTTP returns each response in the POST body,
                    // so a single request can span an entire tool execution.
                    // Bound only the TCP connect with connect_timeout and use
                    // read_timeout as the overall backstop; the per-call
                    // `tokio::time::timeout` in `call_method` is the real cap.
                    let builder = apply_env_proxy(
                        reqwest::Client::builder()
                            .connect_timeout(Duration::from_secs(connect_timeout_secs))
                            .timeout(Duration::from_secs(read_timeout_secs)),
                    );
                    let builder = apply_default_headers(builder, &http_headers)?;
                    let client = builder.build()?;
                    Box::new(StreamableHttpTransport::new(client, url.clone()))
                } else {
                    let builder = apply_env_proxy(
                        reqwest::Client::builder()
                            .timeout(Duration::from_secs(connect_timeout_secs)),
                    );
                    let builder = apply_default_headers(builder, &http_headers)?;
                    let client = builder.build()?;
                    Box::new(
                        SseTransport::connect(client, url.clone(), cancel_token.clone()).await?,
                    )
                }
            }
            McpTransportKind::Stdio => {
                let command = config
                    .command
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("MCP server '{name}' requires a 'command'"))?;
                let mut cmd = super::stdio_spawn::build_stdio_command(
                    command,
                    &config.args,
                    &config.env,
                )
                .with_context(|| {
                    format!(
                        "MCP stdio command resolution failed (server={name} cmd={command:?} args={:?})",
                        config.args,
                    )
                })?;

                let mut child = cmd.spawn().with_context(|| {
                    let env_keys: Vec<&str> = config.env.keys().map(String::as_str).collect();
                    format!(
                        "MCP stdio spawn failed (transport=stdio server={name} cmd={command:?} args={:?} env_keys={env_keys:?}). \
                         On Windows ensure Node.js is installed; try full path to npx.cmd in mcp.json.",
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
            }
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

    /// Send initialize request, negotiate the protocol version, and complete
    /// the handshake with the `initialized` notification.
    ///
    /// We advertise [`PREFERRED_PROTOCOL_VERSION`] and adopt whatever version
    /// the server reports back: if it's one we support we use it directly; if
    /// it's unknown we still proceed with the server's value (best-effort
    /// interop) but log a warning. The negotiated version is handed to the
    /// transport so Streamable HTTP can echo it via `MCP-Protocol-Version`.
    async fn initialize(&mut self) -> Result<()> {
        let init_id = self.next_id();
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "id": init_id,
            "method": "initialize",
            "params": {
                "protocolVersion": PREFERRED_PROTOCOL_VERSION,
                "clientInfo": {
                    "name": "deepseek-runtime",
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

        let response = self.recv(init_id).await?;
        let negotiated = self.negotiate_protocol_version(&response);
        self.transport.set_protocol_version(&negotiated);

        // Send initialized notification (no id, no response expected)
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
        .await?;

        Ok(())
    }

    /// Resolve the effective protocol version from the `initialize` response,
    /// defaulting to our preferred version when the server omits it.
    fn negotiate_protocol_version(&self, response: &serde_json::Value) -> String {
        let server_version = response
            .get("result")
            .and_then(|r| r.get("protocolVersion"))
            .and_then(serde_json::Value::as_str);

        match server_version {
            Some(version) if SUPPORTED_PROTOCOL_VERSIONS.contains(&version) => version.to_string(),
            Some(version) => {
                tracing::warn!(
                    server = %self.name,
                    server_version = version,
                    preferred = PREFERRED_PROTOCOL_VERSION,
                    "MCP server reported an unsupported protocol version; proceeding best-effort"
                );
                version.to_string()
            }
            None => PREFERRED_PROTOCOL_VERSION.to_string(),
        }
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
        let started = std::time::Instant::now();
        let server = self.name.clone();
        let method_name = method.to_string();
        let span = tracing::info_span!(
            "mcp.rpc",
            server = %server,
            method = %method_name,
            timeout_secs
        );

        let outcome = self
            .call_method_inner(method, params, timeout_secs)
            .instrument(span)
            .await;
        let duration_ms = started.elapsed().as_millis() as u64;
        let (success, err_msg, result_bytes) = match &outcome {
            Ok(value) => (
                true,
                None,
                serde_json::to_string(value).map(|s| s.len()).unwrap_or(0),
            ),
            Err(err) => (false, Some(err.to_string()), 0),
        };
        super::observability::record_mcp_call(
            &server,
            &method_name,
            duration_ms,
            success,
            err_msg,
            result_bytes,
        );
        outcome
    }

    async fn call_method_inner(
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
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": call_id,
            "method": method,
            "params": params
        });

        // Bound the whole exchange: for Streamable HTTP the response is
        // delivered in the POST body during `send`, so timing only `recv`
        // would leave long tool calls effectively unbounded.
        let response = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
            self.send(request).await?;
            self.recv(call_id).await
        })
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
