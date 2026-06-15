//! Tui-only runtime extension stored type-erased on core [`Engine::ext`].

use std::sync::Arc;

use tokio::sync::{Mutex as AsyncMutex, mpsc};

use crate::hooks::HookExecutor;
use crate::lsp::LspManager;
use crate::mcp::McpPool;
use crate::tools::approval_cache::ApprovalCache;
use crate::tools::large_output_router::WorkshopVariables;
use crate::tools::shell::SharedShellManager;
use crate::tools::subagent::{SharedSubAgentManager, SubAgentCompletion};

use zagens_runtime_adapters::persist::KernelEventWriter;

use zagens_core::capacity::CapacitySnapshot;
use zagens_core::turn::TurnLoopMode;

use crate::long_horizon::LongHorizonSessionState;

use super::kernel_effect_shadow::KernelEffectShadow;
use super::kernel_guard_shadow::KernelGuardShadow;
use super::kernel_memory_shadow::KernelMemoryShadow;
use super::kernel_projection_shadow::KernelProjectionShadow;
use super::kernel_replay_shadow::KernelReplayShadow;

use super::types::EngineConfigExt;

/// Concrete handles + tui-only config extension bundled for M7 layering.
pub struct EngineRuntimeExt {
    pub config_ext: EngineConfigExt,
    pub long_horizon_state: LongHorizonSessionState,
    pub turn_app_mode: crate::agent_surface::AppMode,
    /// Per-turn LHT mode override from the UI toggle (`None` = use config default).
    pub turn_lht_mode: Option<zagens_core::long_horizon::LhtMode>,
    pub lsp_manager: Arc<LspManager>,
    pub shell_manager: SharedShellManager,
    pub workshop_vars: Option<Arc<AsyncMutex<WorkshopVariables>>>,
    pub subagent_manager: SharedSubAgentManager,
    pub mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
    /// Session-scoped tool approval fingerprints (see `approval_cache`).
    pub approval_cache: ApprovalCache,
    pub tx_subagent_completion: mpsc::UnboundedSender<SubAgentCompletion>,
    /// Shared lock so recv can run concurrently with other engine field access.
    pub rx_subagent_completion: Arc<AsyncMutex<mpsc::UnboundedReceiver<SubAgentCompletion>>>,
    /// Emitted once via `Event::status` when the engine first handles user traffic.
    pub sandbox_init_warning: Option<String>,
    /// Config-driven lifecycle hooks (`[[hooks.hooks]]` in config.toml).
    pub hook_executor: Arc<HookExecutor>,
    /// Whether `SessionStart` has already fired for this engine instance.
    pub session_hooks_started: bool,
    /// Kernel-v2 M3: legacy / shadow / engine policy mode.
    pub tools_policy: crate::config::ToolsPolicyMode,
    /// Kernel-v2 M4: legacy / dag batch scheduler.
    pub tools_scheduler: crate::config::ToolsSchedulerMode,
    /// Per-resource lock slots for DAG fine-grained execution.
    pub resource_lock_registry: Arc<crate::tools::resource_locks::ResourceLockRegistry>,
    /// Phase 3a/3b: append-only KernelEvent double-write (None = disabled).
    pub kernel_event_writer: Option<std::sync::Arc<KernelEventWriter>>,
    /// Phase 3b: live vs projection shadow compare (enabled when writer is Some).
    pub kernel_projection_shadow: KernelProjectionShadow,
    /// Phase 3b: ReplayTurnMachine effect-chain sanity (`[kernel] machine = "shadow"`).
    pub kernel_effect_shadow: KernelEffectShadow,
    /// Phase 3b: guard/continuation projection sanity (`[kernel] machine = "shadow"`).
    pub kernel_guard_shadow: KernelGuardShadow,
    /// Phase 3b: memory-plane projection sanity (`[kernel] machine = "shadow"`).
    pub kernel_memory_shadow: KernelMemoryShadow,
    /// Phase 3b: unified replay coherence (`[kernel] machine = "shadow"`).
    pub kernel_replay_shadow: KernelReplayShadow,
    /// Phase 3b: per-step v3 effect replay parity (`[kernel] machine = "v3"`).
    pub kernel_v3_effect_shadow: super::kernel_v3_effect_shadow::KernelV3EffectShadow,
    /// Resolved `[kernel] machine` kill switch.
    pub kernel_machine_mode: crate::config::KernelMachineMode,
    /// Active turn frame for kernel events emitted outside `run.rs`.
    pub kernel_active_turn_id: Option<String>,
    pub kernel_active_step: u32,
    /// Pending scope for the next v3 `RunCompaction` interpret call.
    pub kernel_run_compaction_scope: Option<super::compaction_ops::RunCompactionScope>,
    /// Stashed capacity checkpoint context consumed by `run_compaction_effect`.
    pub kernel_capacity_snapshot: Option<CapacitySnapshot>,
    pub kernel_capacity_turn_mode: Option<TurnLoopMode>,
    pub kernel_capacity_handoff_reason: Option<String>,
    pub kernel_capacity_intervention_ok: Option<bool>,
    /// Pending IO behind an empty-text v3 `InjectSteer` call (cycle advance).
    pub kernel_pending_inject_steer_kind: Option<super::cycle_briefing_ops::InjectSteerEffectKind>,
    pub kernel_cycle_advance_ok: Option<bool>,
    /// When true, `RunCompaction` / cycle-advance effects record anchors only (no IO).
    pub kernel_effect_replay_anchor_only: bool,
}
