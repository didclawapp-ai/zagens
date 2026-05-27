//! Tool host ports wired by the sidecar at engine spawn (D16 E1-a3).
//!
//! Durable manager handles (`TaskManager`, `AutomationManager`, …) stay in
//! `deepseek-runtime-server` until their ports land here. Metadata and hook
//! injection surfaces are adapter-owned so `tools/` can migrate incrementally.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use deepseek_core::scratchpad::ScratchpadConfig;
use serde_json::Value;

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
