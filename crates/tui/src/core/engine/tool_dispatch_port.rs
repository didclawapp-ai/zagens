//! Bridges the TUI `ToolRegistry` to `deepseek-core::engine::EngineToolDispatch` (P2 PR4).

use std::sync::Arc;

use async_trait::async_trait;
use deepseek_core::engine::{
    tool_call_input, tool_name_is_mutating, tool_result_to_output, EngineToolDispatch,
};
use deepseek_protocol::ToolOutput;
use deepseek_tools::{FunctionCallError, ToolCall};

use crate::tools::ToolRegistry;

pub(crate) use deepseek_core::engine::{
    function_call_to_tool_error, tool_output_to_result, value_to_tool_call,
};

/// Borrowed-registry adapter used by the live turn loop (`execute_tool_with_lock`).
pub(crate) struct RegistryToolDispatch<'a> {
    registry: &'a ToolRegistry,
}

impl<'a> RegistryToolDispatch<'a> {
    #[must_use]
    pub(crate) fn new(registry: &'a ToolRegistry) -> Self {
        Self { registry }
    }
}

/// Owned-registry adapter for handles that outlive a single stack frame.
pub struct TuiEngineToolDispatch {
    registry: Arc<ToolRegistry>,
}

impl TuiEngineToolDispatch {
    #[must_use]
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl EngineToolDispatch for TuiEngineToolDispatch {
    async fn dispatch_tool(
        &self,
        call: ToolCall,
        allow_mutating: bool,
    ) -> Result<ToolOutput, FunctionCallError> {
        RegistryToolDispatch::new(self.registry.as_ref())
            .dispatch_tool(call, allow_mutating)
            .await
    }
}

#[async_trait]
impl EngineToolDispatch for RegistryToolDispatch<'_> {
    async fn dispatch_tool(
        &self,
        call: ToolCall,
        allow_mutating: bool,
    ) -> Result<ToolOutput, FunctionCallError> {
        let name = call.name.clone();
        if !allow_mutating && tool_name_is_mutating(&name) {
            return Err(FunctionCallError::MutatingToolRejected { name });
        }

        let input = tool_call_input(&call)?;
        let result = self
            .registry
            .execute_full(&name, input)
            .await
            .map_err(|err| FunctionCallError::ExecutionFailed {
                name: name.clone(),
                error: deepseek_core::engine::dispatch::format_tool_error(&err, &name),
            })?;
        Ok(tool_result_to_output(result))
    }
}
