//! Low-level tool execution helpers for the engine turn loop.
//!
//! This module keeps the mechanics of MCP dispatch, execution locking, and
//! parallel-tool fanout out of `engine.rs`; the turn loop still owns planning,
//! approval, and how tool results are written back into session state.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use crate::tools::spec::{ToolContext, ToolProgressEmit};
use deepseek_core::engine::{
    tool_progress_opening_line, tool_progress_phase_line, EngineToolDispatch,
};

use super::*;

/// RAII guard that pauses the TUI's terminal-state ownership for the duration
/// of an interactive tool, then restores it on drop.
///
/// Background: interactive tools (anything that needs the raw TTY — external
/// editor, `exec_shell` with stdin, etc.) need the TUI to leave alt-screen,
/// disable raw mode, and release mouse capture so the child sees a normal
/// terminal. The TUI listens for `Event::PauseEvents` / `Event::ResumeEvents`
/// and runs `pause_terminal` / `resume_terminal` in response.
///
/// Earlier code sent `PauseEvents` before tool execution and `ResumeEvents`
/// after. That worked on the happy path, but if the tool's future was dropped
/// — Ctrl+C cancellation, sub-agent abort, parent task cancelled while the
/// tool was awaiting — the second `await` never reached and `ResumeEvents`
/// was never sent. The terminal stayed paused: parent shell scrollbar took
/// over, mouse wheel scrolled the host terminal instead of the transcript,
/// and the TUI rendered as if into a regular cooked-mode buffer.
///
/// `Drop` runs synchronously and can't await, so we use `try_send` on a
/// **clone of the event channel** to push `ResumeEvents` non-blockingly. The
/// engine event channel is the same one we sent `PauseEvents` on, so by the
/// time we drop there is by construction at least one consumed slot, which
/// keeps `try_send` reliable in practice.
pub(super) struct InteractiveTerminalGuard {
    tx: Option<mpsc::Sender<Event>>,
}

impl InteractiveTerminalGuard {
    /// Send `PauseEvents` and arm the guard. If `interactive` is false the
    /// guard is a no-op — `Drop` will skip the resume.
    pub(super) async fn engage(tx: mpsc::Sender<Event>, interactive: bool) -> Self {
        if !interactive {
            return Self { tx: None };
        }
        // Best-effort: if the receiver is gone the TUI has already shut down
        // and there's nothing to restore. Either way we still arm the guard
        // so `Drop` symmetrically tries the resume.
        let _ = tx.send(Event::PauseEvents).await;
        Self { tx: Some(tx) }
    }
}

impl Drop for InteractiveTerminalGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            // Synchronous, non-blocking. If the channel is full we still want
            // the resume to land — log so a cancellation that loses the
            // resume is visible in traces, but don't panic. The TUI also
            // re-sends a resume on its own teardown path as a backstop.
            if let Err(err) = tx.try_send(Event::ResumeEvents) {
                tracing::warn!(
                    target: "engine.tool_execution",
                    ?err,
                    "InteractiveTerminalGuard: try_send(ResumeEvents) failed; \
                     terminal may stay in paused state until the next \
                     pause/resume cycle"
                );
            }
        }
    }
}

pub(crate) use deepseek_core::engine::emit_tool_audit;

async fn emit_tool_progress(tx: &mpsc::Sender<Event>, tool_call_id: &str, message: &str) {
    let text = message.trim_end_matches('\n');
    if text.is_empty() {
        return;
    }
    let line = format!("{text}\n");
    let _ = tx
        .send(Event::ToolCallProgress {
            id: tool_call_id.to_string(),
            output: line,
        })
        .await;
}

/// Bridges shell stdout/stderr polling to `Event::ToolCallProgress`.
struct ChannelToolProgress {
    tx: mpsc::Sender<Event>,
    tool_call_id: String,
    stderr_banner_sent: Arc<AtomicBool>,
}

impl ChannelToolProgress {
    fn new_arc(tx: mpsc::Sender<Event>, tool_call_id: String) -> Arc<Self> {
        Arc::new(Self {
            tx,
            tool_call_id,
            stderr_banner_sent: Arc::new(AtomicBool::new(false)),
        })
    }

    fn emit_raw(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        let _ = self.tx.try_send(Event::ToolCallProgress {
            id: self.tool_call_id.clone(),
            output: text.to_string(),
        });
    }
}

impl ToolProgressEmit for ChannelToolProgress {
    fn emit_stdout(&self, chunk: &str) {
        self.emit_raw(chunk);
    }

    fn emit_stderr(&self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }
        let mut buf = String::new();
        if !self.stderr_banner_sent.swap(true, Ordering::SeqCst) {
            buf.push_str("\n--- stderr ---\n");
        }
        buf.push_str(chunk);
        self.emit_raw(&buf);
    }
}

impl Engine {
    pub(super) async fn execute_mcp_tool_with_pool(
        pool: Arc<AsyncMutex<McpPool>>,
        name: &str,
        input: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let mut pool = pool.lock().await;
        let result = pool
            .call_tool(name, input)
            .await
            .map_err(|e| ToolError::execution_failed(format!("MCP tool failed: {e}")))?;
        let content = serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
        Ok(ToolResult::success(content))
    }

    pub(super) async fn execute_parallel_tool(
        &mut self,
        input: serde_json::Value,
        tool_registry: Option<&crate::tools::ToolRegistry>,
        tool_exec_lock: Arc<RwLock<()>>,
    ) -> Result<ToolResult, ToolError> {
        let calls = parse_parallel_tool_calls(&input)?;
        let mcp_pool = if calls.iter().any(|(tool, _)| McpPool::is_mcp_tool(tool)) {
            Some(self.ensure_mcp_pool().await?)
        } else {
            None
        };
        let Some(registry) = tool_registry else {
            return Err(ToolError::not_available(
                "tool registry unavailable for multi_tool_use.parallel",
            ));
        };

        let mut tasks = FuturesUnordered::new();
        for (tool_name, tool_input) in calls {
            if tool_name == MULTI_TOOL_PARALLEL_NAME {
                return Err(ToolError::invalid_input(
                    "multi_tool_use.parallel cannot call itself",
                ));
            }
            if McpPool::is_mcp_tool(&tool_name) {
                if !mcp_tool_is_parallel_safe(&tool_name) {
                    return Err(ToolError::invalid_input(format!(
                        "Tool '{tool_name}' is an MCP tool and cannot run in parallel. \
                         Allowed MCP tools: list_mcp_resources, list_mcp_resource_templates, \
                         mcp_read_resource, read_mcp_resource, mcp_get_prompt."
                    )));
                }
            } else {
                let Some(spec) = registry.get(&tool_name) else {
                    return Err(ToolError::not_available(format!(
                        "tool '{tool_name}' is not registered"
                    )));
                };
                if !spec.is_read_only() {
                    return Err(ToolError::invalid_input(format!(
                        "Tool '{tool_name}' is not read-only and cannot run in parallel"
                    )));
                }
                if spec.approval_requirement() != ApprovalRequirement::Auto {
                    return Err(ToolError::invalid_input(format!(
                        "Tool '{tool_name}' requires approval and cannot run in parallel"
                    )));
                }
                if !spec.supports_parallel() {
                    return Err(ToolError::invalid_input(format!(
                        "Tool '{tool_name}' does not support parallel execution"
                    )));
                }
            }

            let registry_ref = registry;
            let lock = tool_exec_lock.clone();
            let tx_event = self.tx_event.clone();
            let mcp_pool = mcp_pool.clone();
            tasks.push(async move {
                let result = Engine::execute_tool_with_lock(
                    lock,
                    true,
                    false,
                    tx_event,
                    tool_name.clone(),
                    tool_input.clone(),
                    Some(registry_ref),
                    mcp_pool,
                    None,
                    None,
                )
                .await;
                (tool_name, result)
            });
        }

        let mut results = Vec::new();
        while let Some((tool_name, result)) = tasks.next().await {
            match result {
                Ok(output) => {
                    let mut error = None;
                    if !output.success {
                        error = Some(output.content.clone());
                    }
                    results.push(ParallelToolResultEntry {
                        tool_name,
                        success: output.success,
                        content: output.content,
                        error,
                    });
                }
                Err(err) => {
                    let message = format!("{err}");
                    results.push(ParallelToolResultEntry {
                        tool_name,
                        success: false,
                        content: format!("Error: {message}"),
                        error: Some(message),
                    });
                }
            }
        }

        ToolResult::json(&ParallelToolResult { results })
            .map_err(|e| ToolError::execution_failed(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_tool_with_lock(
        lock: Arc<RwLock<()>>,
        supports_parallel: bool,
        interactive: bool,
        tx_event: mpsc::Sender<Event>,
        tool_name: String,
        tool_input: serde_json::Value,
        registry: Option<&crate::tools::ToolRegistry>,
        mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
        context_override: Option<crate::tools::ToolContext>,
        tool_progress_id: Option<String>,
    ) -> Result<ToolResult, ToolError> {
        let _guard = if supports_parallel {
            ToolExecGuard::Read(lock.read().await)
        } else {
            ToolExecGuard::Write(lock.write().await)
        };

        // RAII pause/resume: ensures `Event::ResumeEvents` always fires on
        // drop, even if the tool future is cancelled mid-await. See
        // `InteractiveTerminalGuard` doc-comment for the regression this
        // closes (parent terminal scrollback hijacking the TUI after a
        // cancelled interactive tool).
        let _terminal = InteractiveTerminalGuard::engage(tx_event.clone(), interactive).await;

        if let Some(ref tid) = tool_progress_id {
            let opening = tool_progress_opening_line(&tool_name, &tool_input);
            emit_tool_progress(&tx_event, tid, &opening).await;
            emit_tool_progress(&tx_event, tid, tool_progress_phase_line(&tool_name)).await;
        }

        if McpPool::is_mcp_tool(&tool_name) {
            if let Some(pool) = mcp_pool {
                Engine::execute_mcp_tool_with_pool(pool, &tool_name, tool_input).await
            } else {
                Err(ToolError::not_available(format!(
                    "tool '{tool_name}' is not registered"
                )))
            }
        } else if let Some(registry) = registry {
            let needs_context_path =
                context_override.is_some() || tool_progress_id.is_some();
            if needs_context_path {
                let merged_ctx: Option<ToolContext> = match tool_progress_id.as_ref() {
                    Some(tid) => {
                        let mut base = match &context_override {
                            Some(co) => co.clone(),
                            None => registry.context().clone(),
                        };
                        base.tool_progress =
                            Some(ChannelToolProgress::new_arc(tx_event.clone(), tid.clone()));
                        Some(base)
                    }
                    None => None,
                };

                let exec_ctx_owned = match merged_ctx {
                    Some(ctx) => Some(ctx),
                    None => context_override,
                };
                registry
                    .execute_full_with_context(&tool_name, tool_input, exec_ctx_owned.as_ref())
                    .await
            } else {
                let call = tool_dispatch_port::value_to_tool_call(tool_name.clone(), tool_input);
                let output = RegistryToolDispatch::new(registry)
                    .dispatch_tool(call, true)
                    .await
                    .map_err(|err| tool_dispatch_port::function_call_to_tool_error(err, &tool_name))?;
                tool_dispatch_port::tool_output_to_result(output)
            }
        } else {
            Err(ToolError::not_available(format!(
                "tool '{tool_name}' is not registered"
            )))
        }
    }
}
