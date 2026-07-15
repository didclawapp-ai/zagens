//! Tool host ports wired by the sidecar at engine spawn (D16 E1-a3+).
//!
//! Durable manager handles (`TaskManager`, `AutomationManager`, …) stay in
//! `zagens-cli`; tools call through these ports so `tools/`
//! can migrate to this crate incrementally.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;
use zagens_core::scratchpad::ScratchpadConfig;

/// Streams incremental tool output to the engine/UI without pulling `Event`
/// into adapters (avoids cycles with orchestrator event types).
pub trait ToolProgressEmit: Send + Sync {
    fn emit_stdout(&self, chunk: &str);
    fn emit_stderr(&self, chunk: &str);
}

/// Shell `shell_env` hook injection port (#456).
pub trait ToolShellEnvHost: Send + Sync {
    fn collect_shell_env(&self, tool_name: &str, tool_args: &Value) -> HashMap<String, String>;
}

/// Durable task operations for model-visible `task_*` / PR-attempt tools (D16 E1-a4).
#[async_trait]
pub trait ToolTaskHost: Send + Sync {
    async fn add_task(&self, req: Value) -> Result<Value, String>;
    async fn list_tasks(&self, limit: Option<usize>) -> Result<Value, String>;
    async fn get_task(&self, task_id: &str) -> Result<Value, String>;
    async fn cancel_task(&self, task_id: &str) -> Result<Value, String>;
    async fn record_tool_metadata(&self, task_id: &str, metadata: &Value) -> Result<(), String>;
    fn artifact_absolute_path(&self, relative: &Path) -> PathBuf;
    fn write_task_artifact(
        &self,
        task_id: &str,
        label: &str,
        content: &str,
    ) -> Result<PathBuf, String>;
}

/// Durable automation operations for model-visible `automation_*` tools (D16 E1-a4).
#[async_trait]
pub trait ToolAutomationHost: Send + Sync {
    async fn create_automation(&self, req: Value) -> Result<Value, String>;
    async fn list_automations(&self) -> Result<Value, String>;
    async fn get_automation(&self, automation_id: &str) -> Result<Value, String>;
    async fn list_runs(&self, automation_id: &str, limit: Option<usize>) -> Result<Value, String>;
    async fn update_automation(&self, automation_id: &str, req: Value) -> Result<Value, String>;
    async fn pause_automation(&self, automation_id: &str) -> Result<Value, String>;
    async fn resume_automation(&self, automation_id: &str) -> Result<Value, String>;
    async fn delete_automation(&self, automation_id: &str) -> Result<Value, String>;
    async fn run_now(&self, automation_id: &str) -> Result<Value, String>;
}

/// Desktop Browser pane host (P1). Sidecar calls this over loopback HTTP when
/// `ZAGENS_BROWSER_BRIDGE_URL` is set; CLI/TUI leave it unset.
#[async_trait]
pub trait ToolBrowserHost: Send + Sync {
    async fn navigate(
        &self,
        thread_id: Option<&str>,
        window_label: Option<&str>,
        url: &str,
    ) -> Result<Value, String>;
    async fn snapshot(
        &self,
        thread_id: Option<&str>,
        window_label: Option<&str>,
    ) -> Result<Value, String>;
    async fn get_text(
        &self,
        thread_id: Option<&str>,
        window_label: Option<&str>,
    ) -> Result<Value, String>;
    async fn console_tail(
        &self,
        thread_id: Option<&str>,
        window_label: Option<&str>,
        limit: usize,
    ) -> Result<Value, String>;
}

/// Durable metadata wired at engine spawn; manager handles stay in sidecar.
#[derive(Clone)]
pub struct RuntimeToolHostWire {
    pub task_data_dir: Option<PathBuf>,
    pub active_task_id: Option<String>,
    pub active_thread_id: Option<String>,
    pub scratchpad_run_id: Arc<Mutex<Option<String>>>,
    pub persist_scratchpad_run_id: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub scratchpad_config: Option<ScratchpadConfig>,
}

impl Default for RuntimeToolHostWire {
    fn default() -> Self {
        Self {
            task_data_dir: None,
            active_task_id: None,
            active_thread_id: None,
            scratchpad_run_id: Arc::new(Mutex::new(None)),
            persist_scratchpad_run_id: None,
            scratchpad_config: None,
        }
    }
}

impl std::fmt::Debug for RuntimeToolHostWire {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeToolHostWire")
            .field("task_data_dir", &self.task_data_dir)
            .field("active_task_id", &self.active_task_id)
            .field("active_thread_id", &self.active_thread_id)
            .field(
                "scratchpad_run_id",
                &self.scratchpad_run_id.lock().ok().and_then(|g| g.clone()),
            )
            .field(
                "persist_scratchpad_run_id",
                &self.persist_scratchpad_run_id.is_some(),
            )
            .field("scratchpad_config", &self.scratchpad_config.is_some())
            .finish()
    }
}
