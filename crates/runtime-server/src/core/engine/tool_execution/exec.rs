//! Sequential tool execution with lock, terminal guard, and progress events.

use std::sync::Arc;

use deepseek_core::engine::{tool_progress_opening_line, tool_progress_phase_line};

use deepseek_core::engine::EngineToolDispatch;

use super::super::*;
use super::progress::{ChannelToolProgress, emit_tool_progress};
use super::terminal_guard::InteractiveTerminalGuard;

impl Engine {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::core::engine) async fn execute_tool_with_lock(
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
            let needs_context_path = context_override.is_some() || tool_progress_id.is_some();
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
                    .map_err(|err| {
                        tool_dispatch_port::function_call_to_tool_error(err, &tool_name)
                    })?;
                tool_dispatch_port::tool_output_to_result(output)
            }
        } else {
            Err(ToolError::not_available(format!(
                "tool '{tool_name}' is not registered"
            )))
        }
    }
}
