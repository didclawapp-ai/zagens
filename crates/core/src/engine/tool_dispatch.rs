//! Tool dispatch boundary for the agent engine (P2 PR4).
//!
//! The live `ToolRegistry` builder stays in `deepseek-runtime`; the engine in core
//! depends on [`EngineToolDispatch`] instead of `crate::tools::*`.

use async_trait::async_trait;
use deepseek_protocol::ToolOutput;
use deepseek_tools::{FunctionCallError, ToolCall};

/// Minimal tool surface required by `Engine` / `turn_loop`.
#[async_trait]
pub trait EngineToolDispatch: Send + Sync {
    async fn dispatch_tool(
        &self,
        call: ToolCall,
        allow_mutating: bool,
    ) -> Result<ToolOutput, FunctionCallError>;
}
