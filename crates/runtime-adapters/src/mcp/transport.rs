use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout};

use super::diagnostics::{ERROR_BODY_PREVIEW_BYTES, bounded_body_excerpt, mask_url_secrets};

// === McpConnection - Async Connection Management ===

// === Transport Trait ===

#[async_trait::async_trait]
pub trait McpTransport: Send + Sync {
    async fn send(&mut self, msg: serde_json::Value) -> Result<()>;
    async fn recv(&mut self) -> Result<serde_json::Value>;

    /// Graceful shutdown — stdio transports send SIGTERM to the child and
    /// give it a brief window to exit before tokio's `kill_on_drop` fires
    /// SIGKILL as the backstop. Default is a no-op for non-stdio transports
    /// that have no child process. Whalescale#420.
    async fn shutdown(&mut self) {}

    /// Inform the transport of the protocol version negotiated during
    /// `initialize`. Streamable HTTP echoes it back via the
    /// `MCP-Protocol-Version` request header on every subsequent call (per the
    /// 2025-06-18 spec); other transports ignore it.
    fn set_protocol_version(&mut self, _version: &str) {}
}

pub struct StdioTransport {
    pub(crate) child: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) reader: tokio::io::BufReader<ChildStdout>,
}

/// How long `StdioTransport::shutdown` waits for the child to exit on SIGTERM
/// before `kill_on_drop` fires SIGKILL. Tuned short so a hung MCP server
/// can't stall TUI exit; well-behaved servers almost always exit within
/// a few hundred ms.
pub(crate) const STDIO_SHUTDOWN_GRACE: Duration = Duration::from_millis(2_000);

/// Best-effort SIGTERM. On Unix uses `libc::kill`; on Windows there's no
/// equivalent so we let `kill_on_drop` (TerminateProcess) handle it via the
/// subsequent Drop. Returns whether a signal was actually sent.
fn send_sigterm(child: &Child) -> bool {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            // SAFETY: pid was just obtained from `child.id()`. `libc::kill`
            // with `SIGTERM` is async-signal-safe and never observes invalid
            // memory. Worst case (pid wrap / process already gone) returns
            // ESRCH, which we deliberately ignore.
            unsafe {
                let _ = libc::kill(pid as i32, libc::SIGTERM);
            }
            return true;
        }
        false
    }
    #[cfg(not(unix))]
    {
        let _ = child;
        false
    }
}

#[async_trait::async_trait]
impl McpTransport for StdioTransport {
    async fn send(&mut self, msg: serde_json::Value) -> Result<()> {
        let line = serde_json::to_string(&msg)? + "\n";
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<serde_json::Value> {
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = self.reader.read_line(&mut line).await?;
            if bytes == 0 {
                anyhow::bail!("Stdio transport closed");
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
                return Ok(value);
            }
        }
    }

    /// Send SIGTERM and wait up to `STDIO_SHUTDOWN_GRACE` for graceful exit
    /// before letting Drop / `kill_on_drop` fire SIGKILL as the backstop.
    async fn shutdown(&mut self) {
        send_sigterm(&self.child);
        // Give the child a window to exit cleanly. Discard the result —
        // either it exits (success) or the timeout fires (Drop will SIGKILL).
        let _ = tokio::time::timeout(STDIO_SHUTDOWN_GRACE, self.child.wait()).await;
    }
}

/// Drop fallback (#420): if `shutdown` was never called explicitly, still
/// fire SIGTERM before tokio's `kill_on_drop` sends SIGKILL. The two
/// signals arrive back-to-back so well-behaved servers at least see the
/// SIGTERM first; misbehaving ones get SIGKILL'd anyway.
impl Drop for StdioTransport {
    fn drop(&mut self) {
        send_sigterm(&self.child);
    }
}

pub struct SseTransport {
    client: reqwest::Client,
    base_url: String,
    endpoint_url: Option<String>,
    receiver: tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
}

impl SseTransport {
    pub async fn connect(
        client: reqwest::Client,
        url: String,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let client_clone = client.clone();
        let url_clone = url.clone();

        tokio::spawn(async move {
            if cancel_token.is_cancelled() {
                return;
            }
            use futures_util::FutureExt;
            let result = std::panic::AssertUnwindSafe(Self::run_sse_loop(
                client_clone,
                url_clone,
                tx,
                cancel_token,
            ))
            .catch_unwind()
            .await;
            match result {
                Ok(res) => {
                    if let Err(e) = res {
                        tracing::error!("SSE loop error: {}", e);
                    }
                }
                Err(panic_err) => {
                    if let Some(msg) = panic_err.downcast_ref::<&str>() {
                        tracing::error!("SSE loop panicked: {}", msg);
                    } else if let Some(msg) = panic_err.downcast_ref::<String>() {
                        tracing::error!("SSE loop panicked: {}", msg);
                    } else {
                        tracing::error!("SSE loop panicked with unknown error");
                    }
                }
            }
        });

        Ok(Self {
            client,
            base_url: url,
            endpoint_url: None,
            receiver: rx,
        })
    }

    async fn run_sse_loop(
        client: reqwest::Client,
        url: String,
        tx: tokio::sync::mpsc::UnboundedSender<serde_json::Value>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<()> {
        let response = client.get(&url).send().await.with_context(|| {
            format!(
                "MCP SSE connect failed (transport=http url={})",
                mask_url_secrets(&url),
            )
        })?;
        let status = response.status();
        if !status.is_success() {
            let body_excerpt = bounded_body_excerpt(response, ERROR_BODY_PREVIEW_BYTES).await;
            anyhow::bail!(
                "MCP SSE rejected (transport=http url={} status={}): {}",
                mask_url_secrets(&url),
                status,
                body_excerpt,
            );
        }

        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;
        let mut buffer = String::new();

        loop {
            if cancel_token.is_cancelled() {
                tracing::debug!("SSE loop cancelled");
                break;
            }
            let item = tokio::select! {
                _ = cancel_token.cancelled() => {
                    tracing::debug!("SSE loop shutting down");
                    break;
                }
                item = stream.next() => {
                    match item {
                        Some(i) => i,
                        None => break,
                    }
                }
            };
            let chunk = item?;
            let s = String::from_utf8_lossy(&chunk);
            buffer.push_str(&s);

            while let Some(pos) = buffer.find("\n\n") {
                let event_block = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                let mut event_type = "message";
                let mut data = String::new();

                for line in event_block.lines() {
                    if let Some(stripped) = line.strip_prefix("event: ") {
                        event_type = stripped;
                    } else if let Some(stripped) = line.strip_prefix("data: ") {
                        data.push_str(stripped);
                    }
                }

                match event_type {
                    "endpoint" => {
                        // Special internal message to set endpoint
                        let _ = tx.send(serde_json::json!({
                            "__internal_sse_endpoint__": data
                        }));
                    }
                    "message" => {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&data) {
                            let _ = tx.send(val);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl McpTransport for SseTransport {
    async fn send(&mut self, msg: serde_json::Value) -> Result<()> {
        let endpoint = self
            .endpoint_url
            .as_ref()
            .context("SSE endpoint not yet discovered")?;
        let response = self.client.post(endpoint).json(&msg).send().await?;
        if !response.status().is_success() {
            anyhow::bail!("Failed to send message via SSE POST: {}", response.status());
        }
        Ok(())
    }

    async fn recv(&mut self) -> Result<serde_json::Value> {
        loop {
            let msg = self.receiver.recv().await.context("SSE transport closed")?;
            if let Some(endpoint) = msg.get("__internal_sse_endpoint__") {
                let url_str = endpoint.as_str().context("Invalid endpoint format")?;
                // Handle relative vs absolute URLs
                if url_str.starts_with("http") {
                    self.endpoint_url = Some(url_str.to_string());
                } else {
                    let base = reqwest::Url::parse(&self.base_url)?;
                    let joined = base.join(url_str)?;
                    self.endpoint_url = Some(joined.to_string());
                }
                continue;
            }
            return Ok(msg);
        }
    }
}

/// 2025 Streamable HTTP transport (spec rev `2025-03-26`/`2025-06-18`).
///
/// Unlike the legacy [`SseTransport`] (separate GET stream + POST endpoint),
/// Streamable HTTP uses a **single** endpoint: every JSON-RPC message is
/// `POST`ed, and the server replies either with `application/json` (a single
/// response) or `text/event-stream` (an SSE batch that may interleave
/// server notifications before the response). Responses are buffered into
/// `inbox`; the connection's `recv` loop drains them and matches by id.
///
/// Session continuity is maintained via the `Mcp-Session-Id` header: we
/// capture whatever the server returns (typically on `initialize`) and echo
/// it back on subsequent requests. We do not open the optional standalone GET
/// stream for server-initiated messages — request/response over POST covers
/// tool discovery and invocation.
pub struct StreamableHttpTransport {
    client: reqwest::Client,
    url: String,
    session_id: Option<String>,
    protocol_version: Option<String>,
    pub(super) inbox: std::collections::VecDeque<serde_json::Value>,
}

impl StreamableHttpTransport {
    pub fn new(client: reqwest::Client, url: String) -> Self {
        Self {
            client,
            url,
            session_id: None,
            protocol_version: None,
            inbox: std::collections::VecDeque::new(),
        }
    }

    /// Parse an SSE response body, enqueuing every `message` event's JSON data.
    pub(super) fn enqueue_sse_body(&mut self, body: &str) {
        for block in body.split("\n\n") {
            let mut event_type = "message";
            let mut data = String::new();
            for line in block.lines() {
                if let Some(rest) = line.strip_prefix("event:") {
                    event_type = rest.trim();
                } else if let Some(rest) = line.strip_prefix("data:") {
                    if !data.is_empty() {
                        data.push('\n');
                    }
                    data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
                }
            }
            if event_type == "message"
                && !data.is_empty()
                && let Ok(value) = serde_json::from_str::<serde_json::Value>(&data)
            {
                self.inbox.push_back(value);
            }
        }
    }

    /// Enqueue a JSON body, flattening a top-level array (JSON-RPC batch).
    pub(super) fn enqueue_json_body(&mut self, body: &str) {
        match serde_json::from_str::<serde_json::Value>(body) {
            Ok(serde_json::Value::Array(items)) => {
                for item in items {
                    self.inbox.push_back(item);
                }
            }
            Ok(value) => self.inbox.push_back(value),
            Err(_) => {}
        }
    }
}

#[async_trait::async_trait]
impl McpTransport for StreamableHttpTransport {
    async fn send(&mut self, msg: serde_json::Value) -> Result<()> {
        let mut req = self
            .client
            .post(&self.url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(
                reqwest::header::ACCEPT,
                "application/json, text/event-stream",
            );
        if let Some(session) = &self.session_id {
            req = req.header("Mcp-Session-Id", session);
        }
        if let Some(version) = &self.protocol_version {
            req = req.header("MCP-Protocol-Version", version);
        }

        let response = req.json(&msg).send().await.with_context(|| {
            format!(
                "MCP Streamable HTTP POST failed (transport=http url={})",
                mask_url_secrets(&self.url),
            )
        })?;

        // Capture/refresh the session id from any response (set on initialize).
        if let Some(session) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            self.session_id = Some(session.to_string());
        }

        let status = response.status();
        if !status.is_success() {
            let body = bounded_body_excerpt(response, ERROR_BODY_PREVIEW_BYTES).await;
            anyhow::bail!(
                "MCP Streamable HTTP rejected (transport=http url={} status={}): {}",
                mask_url_secrets(&self.url),
                status,
                body,
            );
        }

        // 202 Accepted is the spec's reply to notifications/responses that
        // carry no body — nothing to enqueue.
        if status == reqwest::StatusCode::ACCEPTED {
            return Ok(());
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let body = response
            .text()
            .await
            .context("MCP Streamable HTTP: failed to read response body")?;
        if content_type.contains("text/event-stream") {
            self.enqueue_sse_body(&body);
        } else if !body.trim().is_empty() {
            self.enqueue_json_body(&body);
        }
        Ok(())
    }

    async fn recv(&mut self) -> Result<serde_json::Value> {
        self.inbox
            .pop_front()
            .context("MCP Streamable HTTP: no message available from server")
    }

    fn set_protocol_version(&mut self, version: &str) {
        self.protocol_version = Some(version.to_string());
    }
}
