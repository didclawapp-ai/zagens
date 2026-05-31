//! Sidecar `RuntimeThreadMonitorHost` — panel SSE, artifacts, sandbox policy (D16 E1-b phase 3).

use std::path::PathBuf;

use async_trait::async_trait;
use deepseek_tools::{ToolError, ToolResult};
use serde_json::Value;

use super::manager::RuntimeThreadManager;
use deepseek_runtime_orchestrator::runtime_threads::{
    RuntimeThreadMonitorHost, checklist_tool_needs_panel_push,
    scratchpad_tool_needs_panel_push,
};

#[async_trait]
impl RuntimeThreadMonitorHost<super::RuntimeEnginePolicy, super::RuntimeUserInputResponse>
    for RuntimeThreadManager
{
    fn full_access_sandbox_policy(&self) -> super::RuntimeEnginePolicy {
        crate::sandbox::SandboxPolicy::DangerFullAccess
    }

    fn artifact_refs_from_tool_output(
        &self,
        session_id: Option<&str>,
        content: &str,
        metadata: Option<&Value>,
    ) -> Vec<PathBuf> {
        crate::tools::large_output_router::artifact_refs_from_tool_output(
            session_id,
            content,
            metadata,
        )
    }

    async fn after_message_complete_panels(&self, thread_id: &str, turn_id: &str) {
        let _ = self.emit_panel_context(thread_id, turn_id).await;
    }

    async fn after_turn_complete_panels(&self, thread_id: &str, turn_id: &str) {
        let _ = self.emit_panel_context(thread_id, turn_id).await;
        let _ = self.emit_panel_scratchpad(thread_id, turn_id).await;
        let _ = self.emit_panel_checklist(thread_id, turn_id).await;
        let _ = self.emit_panel_harness_task_graph(thread_id, turn_id).await;
    }

    async fn after_tool_call_complete_panels(
        &self,
        thread_id: &str,
        turn_id: &str,
        tool_name: &str,
        result: &Result<ToolResult, ToolError>,
    ) {
        if tool_name == "update_plan" {
            if let Ok(output) = result {
                if output.success {
                    if let Some(meta) = &output.metadata {
                        if let Some(plan) = meta
                            .get("task_updates")
                            .and_then(|u| u.get("plan"))
                        {
                            if let Ok(json_str) = serde_json::to_string(plan) {
                                self.persist_thread_plan(thread_id, &json_str);
                                let _ = self.emit_panel_plan(thread_id, turn_id).await;
                                let _ = self
                                    .emit_panel_harness_task_graph(thread_id, turn_id)
                                    .await;
                            }
                        }
                    }
                }
            }
        }
        if matches!(
            tool_name,
            "checklist_write"
                | "checklist_add"
                | "checklist_update"
                | "todo_write"
                | "todo_add"
                | "todo_update"
        ) {
            if let Ok(output) = result {
                if output.success {
                    if let Some(meta) = &output.metadata {
                        if let Some(task_updates) = meta.get("task_updates") {
                            if let Some(checklist_json) = task_updates.get("checklist") {
                                if let Ok(json_str) = serde_json::to_string(checklist_json) {
                                    self.persist_thread_checklist(thread_id, &json_str);
                                    let _ = self.emit_panel_checklist(thread_id, turn_id).await;
                                    let _ = self
                                        .emit_panel_harness_task_graph(thread_id, turn_id)
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
        }
        if checklist_tool_needs_panel_push(tool_name) {
            let _ = self.emit_panel_checklist(thread_id, turn_id).await;
        }
        if scratchpad_tool_needs_panel_push(tool_name)
            && result.as_ref().is_ok_and(|o| o.success)
        {
            let _ = self.emit_panel_scratchpad(thread_id, turn_id).await;
        }
    }

    async fn observe_harness_status(&self, thread_id: &str, turn_id: &str, message: &str) {
        // Authoritative checklist snapshot pushed by the engine on every
        // successful checklist mutation (CCR progress-desync fix). Reconcile the
        // persisted/cached checklist — the source the desktop UI reads — to the
        // engine's truth, then re-emit the checklist + task-graph panels. This
        // closes the gap where the monitor's per-tool persistence missed a
        // checklist mutation and left the UI frozen mid-task.
        if let Some(checklist_json) = message.strip_prefix("long_horizon.checklist_persist:") {
            self.persist_thread_checklist(thread_id, checklist_json);
            let _ = self.emit_panel_checklist(thread_id, turn_id).await;
            let _ = self.emit_panel_harness_task_graph(thread_id, turn_id).await;
            return;
        }
        if self.update_harness_telemetry_from_status(thread_id, message) {
            let _ = self.emit_panel_harness_task_graph(thread_id, turn_id).await;
        }
    }
}
