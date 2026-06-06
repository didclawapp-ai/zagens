//! Sidecar implementations of adapter tool host ports (D16 E1-a3+).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use deepseek_runtime_adapters::tools::{ToolAutomationHost, ToolShellEnvHost, ToolTaskHost};
use serde_json::Value;

use crate::automation_manager::SharedAutomationManager;
use crate::hooks::{HookContext, HookExecutor};
use crate::task_manager::{NewTaskRequest, SharedTaskManager, TaskRecord};

/// Bridges `HookExecutor` to the adapter `ToolShellEnvHost` port.
pub struct HookShellEnvHost(pub Arc<HookExecutor>);

impl ToolShellEnvHost for HookShellEnvHost {
    fn collect_shell_env(&self, tool_name: &str, tool_args: &Value) -> HashMap<String, String> {
        let hook_ctx = HookContext::new()
            .with_tool_name(tool_name)
            .with_tool_args(tool_args);
        self.0.collect_shell_env(&hook_ctx)
    }
}

/// Bridges `TaskManager` to the adapter `ToolTaskHost` port.
pub struct TaskManagerHost(pub SharedTaskManager);

#[async_trait]
impl ToolTaskHost for TaskManagerHost {
    async fn add_task(&self, req: Value) -> Result<Value, String> {
        let req: NewTaskRequest =
            serde_json::from_value(req).map_err(|e| format!("invalid task request: {e}"))?;
        let task = self.0.add_task(req).await.map_err(|e| e.to_string())?;
        serde_json::to_value(task).map_err(|e| e.to_string())
    }

    async fn list_tasks(&self, limit: Option<usize>) -> Result<Value, String> {
        let tasks = self.0.list_tasks(limit).await;
        serde_json::to_value(tasks).map_err(|e| e.to_string())
    }

    async fn get_task(&self, task_id: &str) -> Result<Value, String> {
        let task = self.0.get_task(task_id).await.map_err(|e| e.to_string())?;
        serde_json::to_value(task).map_err(|e| e.to_string())
    }

    async fn cancel_task(&self, task_id: &str) -> Result<Value, String> {
        let task = self
            .0
            .cancel_task(task_id)
            .await
            .map_err(|e| e.to_string())?;
        serde_json::to_value(task).map_err(|e| e.to_string())
    }

    async fn record_tool_metadata(&self, task_id: &str, metadata: &Value) -> Result<(), String> {
        self.0
            .record_tool_metadata(task_id, metadata)
            .await
            .map(|_: TaskRecord| ())
            .map_err(|e| e.to_string())
    }

    fn artifact_absolute_path(&self, relative: &Path) -> PathBuf {
        self.0.artifact_absolute_path(relative)
    }

    fn write_task_artifact(
        &self,
        task_id: &str,
        label: &str,
        content: &str,
    ) -> Result<PathBuf, String> {
        self.0
            .write_task_artifact(task_id, label, content)
            .map_err(|e| e.to_string())
    }
}

/// Bridges `AutomationManager` to the adapter `ToolAutomationHost` port.
pub struct AutomationManagerHost {
    pub automations: SharedAutomationManager,
    pub tasks: SharedTaskManager,
}

#[async_trait]
impl ToolAutomationHost for AutomationManagerHost {
    async fn create_automation(&self, req: Value) -> Result<Value, String> {
        let req: crate::automation_manager::CreateAutomationRequest =
            serde_json::from_value(req).map_err(|e| format!("invalid automation request: {e}"))?;
        let manager = self.automations.lock().await;
        let automation = manager.create_automation(req).map_err(|e| e.to_string())?;
        serde_json::to_value(automation).map_err(|e| e.to_string())
    }

    async fn list_automations(&self) -> Result<Value, String> {
        let manager = self.automations.lock().await;
        let automations = manager.list_automations().map_err(|e| e.to_string())?;
        serde_json::to_value(automations).map_err(|e| e.to_string())
    }

    async fn get_automation(&self, automation_id: &str) -> Result<Value, String> {
        let manager = self.automations.lock().await;
        let automation = manager
            .get_automation(automation_id)
            .map_err(|e| e.to_string())?;
        serde_json::to_value(automation).map_err(|e| e.to_string())
    }

    async fn list_runs(&self, automation_id: &str, limit: Option<usize>) -> Result<Value, String> {
        let manager = self.automations.lock().await;
        let runs = manager
            .list_runs(automation_id, limit)
            .map_err(|e| e.to_string())?;
        serde_json::to_value(runs).map_err(|e| e.to_string())
    }

    async fn update_automation(&self, automation_id: &str, req: Value) -> Result<Value, String> {
        let req: crate::automation_manager::UpdateAutomationRequest =
            serde_json::from_value(req).map_err(|e| format!("invalid automation update: {e}"))?;
        let manager = self.automations.lock().await;
        let automation = manager
            .update_automation(automation_id, req)
            .map_err(|e| e.to_string())?;
        serde_json::to_value(automation).map_err(|e| e.to_string())
    }

    async fn pause_automation(&self, automation_id: &str) -> Result<Value, String> {
        let manager = self.automations.lock().await;
        let automation = manager
            .pause_automation(automation_id)
            .map_err(|e| e.to_string())?;
        serde_json::to_value(automation).map_err(|e| e.to_string())
    }

    async fn resume_automation(&self, automation_id: &str) -> Result<Value, String> {
        let manager = self.automations.lock().await;
        let automation = manager
            .resume_automation(automation_id)
            .map_err(|e| e.to_string())?;
        serde_json::to_value(automation).map_err(|e| e.to_string())
    }

    async fn delete_automation(&self, automation_id: &str) -> Result<Value, String> {
        let manager = self.automations.lock().await;
        let automation = manager
            .delete_automation(automation_id)
            .map_err(|e| e.to_string())?;
        serde_json::to_value(automation).map_err(|e| e.to_string())
    }

    async fn run_now(&self, automation_id: &str) -> Result<Value, String> {
        let manager = self.automations.lock().await;
        let run = manager
            .run_now(automation_id, &self.tasks)
            .await
            .map_err(|e| e.to_string())?;
        serde_json::to_value(run).map_err(|e| e.to_string())
    }
}
