//! Sidecar implementations of adapter tool host ports (D16 E1-a3+).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use zagens_runtime_adapters::tools::{
    ToolAutomationHost, ToolBrowserHost, ToolShellEnvHost, ToolTaskHost,
};

use crate::automation_manager::SharedAutomationManager;
use crate::hooks::HookExecutor;
use crate::task_manager::{NewTaskRequest, SharedTaskManager, TaskRecord};

/// Bridges `HookExecutor` to the adapter `ToolShellEnvHost` port.
pub struct HookShellEnvHost(pub Arc<HookExecutor>);

impl ToolShellEnvHost for HookShellEnvHost {
    fn collect_shell_env(&self, tool_name: &str, tool_args: &Value) -> HashMap<String, String> {
        let hook_ctx = self
            .0
            .base_context()
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
        let config = crate::config::Config::load(None, None).map_err(|e| e.to_string())?;
        let manager = self.automations.lock().await;
        let run = manager
            .run_now(
                automation_id,
                &config,
                &self.tasks,
                self.automations.clone(),
            )
            .await
            .map_err(|e| e.to_string())?;
        serde_json::to_value(run).map_err(|e| e.to_string())
    }
}

/// HTTP client for the desktop Browser loopback bridge (`ZAGENS_BROWSER_BRIDGE_URL`).
pub struct HttpBrowserHost {
    base_url: String,
    token: String,
    client: reqwest::Client,
}

impl HttpBrowserHost {
    pub fn from_env() -> Option<Self> {
        let base = std::env::var("ZAGENS_BROWSER_BRIDGE_URL")
            .ok()?
            .trim()
            .to_string();
        if base.is_empty() {
            return None;
        }
        let token = std::env::var("DEEPSEEK_RUNTIME_TOKEN").unwrap_or_default();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .ok()?;
        Some(Self {
            base_url: base.trim_end_matches('/').to_string(),
            token,
            client,
        })
    }

    async fn call_op(&self, body: Value) -> Result<Value, String> {
        let resp = self
            .client
            .post(format!("{}/v1/browser/op", self.base_url))
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("browser bridge request failed: {e}"))?;
        let status = resp.status();
        let payload: Value = resp
            .json()
            .await
            .map_err(|e| format!("browser bridge bad json: {e}"))?;
        if !status.is_success() {
            return Err(payload.to_string());
        }
        let ok = payload.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        if ok {
            return Ok(payload.get("result").cloned().unwrap_or(Value::Null));
        }
        if let Some(err) = payload.get("error") {
            return Err(err.to_string());
        }
        Err(payload.to_string())
    }

    fn base_body(
        op: &str,
        thread_id: Option<&str>,
        window_label: Option<&str>,
    ) -> serde_json::Map<String, Value> {
        let mut m = serde_json::Map::new();
        m.insert("op".into(), Value::String(op.into()));
        if let Some(t) = thread_id {
            m.insert("threadId".into(), Value::String(t.into()));
        }
        if let Some(w) = window_label {
            m.insert("windowLabel".into(), Value::String(w.into()));
        }
        m
    }
}

#[async_trait]
impl ToolBrowserHost for HttpBrowserHost {
    async fn navigate(
        &self,
        thread_id: Option<&str>,
        window_label: Option<&str>,
        url: &str,
    ) -> Result<Value, String> {
        let mut body = Self::base_body("navigate", thread_id, window_label);
        body.insert("url".into(), Value::String(url.into()));
        self.call_op(Value::Object(body)).await
    }

    async fn snapshot(
        &self,
        thread_id: Option<&str>,
        window_label: Option<&str>,
        include_screenshot: bool,
    ) -> Result<Value, String> {
        let mut body = Self::base_body("snapshot", thread_id, window_label);
        if include_screenshot {
            body.insert("includeScreenshot".into(), Value::Bool(true));
        }
        self.call_op(Value::Object(body)).await
    }

    async fn get_text(
        &self,
        thread_id: Option<&str>,
        window_label: Option<&str>,
    ) -> Result<Value, String> {
        self.call_op(Value::Object(Self::base_body(
            "get_text",
            thread_id,
            window_label,
        )))
        .await
    }

    async fn console_tail(
        &self,
        thread_id: Option<&str>,
        window_label: Option<&str>,
        limit: usize,
    ) -> Result<Value, String> {
        let mut body = Self::base_body("console_tail", thread_id, window_label);
        body.insert("limit".into(), Value::from(limit as u64));
        self.call_op(Value::Object(body)).await
    }

    async fn click(
        &self,
        thread_id: Option<&str>,
        window_label: Option<&str>,
        element_ref: &str,
    ) -> Result<Value, String> {
        let mut body = Self::base_body("click", thread_id, window_label);
        body.insert("ref".into(), Value::String(element_ref.into()));
        self.call_op(Value::Object(body)).await
    }

    async fn type_text(
        &self,
        thread_id: Option<&str>,
        window_label: Option<&str>,
        element_ref: &str,
        text: &str,
    ) -> Result<Value, String> {
        let mut body = Self::base_body("type", thread_id, window_label);
        body.insert("ref".into(), Value::String(element_ref.into()));
        body.insert("text".into(), Value::String(text.into()));
        self.call_op(Value::Object(body)).await
    }

    async fn scroll(
        &self,
        thread_id: Option<&str>,
        window_label: Option<&str>,
        element_ref: Option<&str>,
        direction: &str,
        amount: Option<f64>,
    ) -> Result<Value, String> {
        let mut body = Self::base_body("scroll", thread_id, window_label);
        body.insert("direction".into(), Value::String(direction.into()));
        if let Some(r) = element_ref {
            body.insert("ref".into(), Value::String(r.into()));
        }
        if let Some(a) = amount {
            body.insert("amount".into(), json!(a));
        }
        self.call_op(Value::Object(body)).await
    }

    async fn start_preview(
        &self,
        thread_id: Option<&str>,
        window_label: Option<&str>,
        workspace: Option<&str>,
    ) -> Result<Value, String> {
        let mut body = Self::base_body("start_preview", thread_id, window_label);
        if let Some(w) = workspace {
            body.insert("workspace".into(), Value::String(w.into()));
        }
        self.call_op(Value::Object(body)).await
    }
}
