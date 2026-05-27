//! Background task / automation slots for engine spawn (D16 E1-b phase 5).

use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use deepseek_runtime_orchestrator::runtime_threads::types::ThreadRecord;
use deepseek_runtime_adapters::tools::RuntimeToolHostWire;

use crate::automation_manager::SharedAutomationManager;
use crate::task_manager::SharedTaskManager;
use crate::tools::spec::RuntimeToolServices;

/// Sidecar-only slots wired into `RuntimeToolServices` at engine spawn.
#[derive(Clone, Default)]
pub struct RuntimeThreadBackgroundSlots {
    task_manager: Arc<StdMutex<Option<SharedTaskManager>>>,
    automations: Arc<StdMutex<Option<SharedAutomationManager>>>,
}

impl RuntimeThreadBackgroundSlots {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attach_task_manager(&self, task_manager: SharedTaskManager) {
        if let Ok(mut slot) = self.task_manager.lock() {
            *slot = Some(task_manager);
        }
    }

    pub fn attach_automation_manager(&self, automations: SharedAutomationManager) {
        if let Ok(mut slot) = self.automations.lock() {
            *slot = Some(automations);
        }
    }

    #[must_use]
    pub fn build_runtime_tool_services(
        &self,
        thread: &ThreadRecord,
        task_data_dir: PathBuf,
        scratchpad_run_id_slot: Arc<StdMutex<Option<String>>>,
        persist_scratchpad_run_id: Arc<dyn Fn(String) + Send + Sync>,
        scratchpad_config: crate::scratchpad::ScratchpadConfig,
    ) -> RuntimeToolServices {
        RuntimeToolServices {
            wire: RuntimeToolHostWire {
                task_data_dir: Some(task_data_dir),
                active_task_id: thread.task_id.clone(),
                active_thread_id: Some(thread.id.clone()),
                scratchpad_run_id: scratchpad_run_id_slot,
                persist_scratchpad_run_id: Some(persist_scratchpad_run_id),
                scratchpad_config: Some(scratchpad_config),
            },
            shell_manager: None,
            task_manager: self.task_manager.lock().ok().and_then(|slot| slot.clone()),
            automations: self.automations.lock().ok().and_then(|slot| slot.clone()),
            shell_env: None,
        }
    }
}
