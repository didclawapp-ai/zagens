//! Host bundle for [`super::runtime::Engine::with_hosts`] (M7).

use std::sync::Arc;

use crate::capacity::CapacityController;
use crate::chat::LlmClient;
use crate::engine::hosts::{
    LspHost, SandboxHost, SeamHost, ShellHost, TopicMemoryHost, WorkshopHost,
};
use crate::engine::platform_ext::EnginePlatformExt;

/// Subsystem hosts wired by the tui-side builder before core `Engine` construction.
pub struct EngineHostBundle<P, R> {
    pub lsp: Arc<dyn LspHost>,
    pub shell: Box<dyn ShellHost>,
    pub sandbox: Box<dyn SandboxHost>,
    pub seam: Option<Box<dyn SeamHost>>,
    pub workshop: Option<Box<dyn WorkshopHost>>,
    pub topic_memory: Box<dyn TopicMemoryHost>,
    pub capacity_controller: CapacityController,
    pub deepseek_client: Option<Arc<dyn LlmClient>>,
    pub deepseek_client_error: Option<String>,
    pub api_key_env_only_recovery: Option<String>,
    /// Tui platform extension (`EngineRuntimeExt` + op dispatch).
    pub ext: Box<dyn EnginePlatformExt<P, R>>,
    pub scratchpad_run_id: Option<String>,
}
