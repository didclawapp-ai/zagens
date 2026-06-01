//! Tui-only runtime extension stored type-erased on core [`Engine::ext`].

use std::sync::Arc;

use tokio::sync::{Mutex as AsyncMutex, mpsc};

use crate::lsp::LspManager;
use crate::mcp::McpPool;
use crate::tools::large_output_router::WorkshopVariables;
use crate::tools::shell::SharedShellManager;
use crate::tools::subagent::{SharedSubAgentManager, SubAgentCompletion};

use crate::long_horizon::LongHorizonSessionState;

use super::types::EngineConfigExt;

/// Concrete handles + tui-only config extension bundled for M7 layering.
pub struct EngineRuntimeExt {
    pub config_ext: EngineConfigExt,
    pub long_horizon_state: LongHorizonSessionState,
    pub turn_app_mode: crate::agent_surface::AppMode,
    /// Per-turn LHT mode override from the UI toggle (`None` = use config default).
    pub turn_lht_mode: Option<deepseek_core::long_horizon::LhtMode>,
    pub lsp_manager: Arc<LspManager>,
    pub shell_manager: SharedShellManager,
    pub workshop_vars: Option<Arc<AsyncMutex<WorkshopVariables>>>,
    pub subagent_manager: SharedSubAgentManager,
    pub mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
    pub tx_subagent_completion: mpsc::UnboundedSender<SubAgentCompletion>,
    /// Shared lock so recv can run concurrently with other engine field access.
    pub rx_subagent_completion: Arc<AsyncMutex<mpsc::UnboundedReceiver<SubAgentCompletion>>>,
    /// Emitted once via `Event::status` when the engine first handles user traffic.
    pub sandbox_init_warning: Option<String>,
}
