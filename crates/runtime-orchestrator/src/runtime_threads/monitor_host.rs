//! Sidecar hooks for the turn monitor loop (D16 E1-b phase 3).

use std::path::PathBuf;

use async_trait::async_trait;
use deepseek_tools::{ToolError, ToolResult};
use serde_json::Value;

/// Host port for Zagens panel SSE, artifact refs, and sandbox elevation policy.
#[async_trait]
pub trait RuntimeThreadMonitorHost<P, R>: Send + Sync + Clone
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
{
    fn full_access_sandbox_policy(&self) -> P;

    fn artifact_refs_from_tool_output(
        &self,
        session_id: Option<&str>,
        content: &str,
        metadata: Option<&Value>,
    ) -> Vec<PathBuf>;

    async fn after_message_complete_panels(&self, thread_id: &str, turn_id: &str);

    async fn after_turn_complete_panels(&self, thread_id: &str, turn_id: &str);

    async fn after_tool_call_complete_panels(
        &self,
        thread_id: &str,
        turn_id: &str,
        tool_name: &str,
        result: &Result<ToolResult, ToolError>,
    );
}
