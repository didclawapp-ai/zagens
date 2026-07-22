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

use super::kernel_outer_boundary::V3OuterBoundaryTurnGrants;
use super::kernel_turn_event_buffer::KernelTurnEventBuffer;
use super::kernel_turn_replay_verify::KernelTurnReplayVerify;
use super::kernel_v3_step_verify::KernelV3StepVerify;

use super::types::EngineConfigExt;

/// Concrete handles + tui-only config extension bundled for M7 layering.
pub struct EngineRuntimeExt {
    pub config_ext: EngineConfigExt,
    pub long_horizon_state: LongHorizonSessionState,
    /// Soft Explore/Edit/Verify/Ship phase for daily Agent catalog deferral.
    pub agent_tool_phase: zagens_core::engine::AgentToolPhase,
    /// Failure-driven eager-tool hot-start (compile fail / symbol missing).
    pub failure_hot_start: zagens_core::engine::FailureHotStart,
    /// Differential read anchors (last edit / tool_use citation windows).
    pub diff_read_anchors: std::sync::Arc<
        tokio::sync::Mutex<zagens_core::engine::DiffReadAnchors>,
    >,
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
    /// TS-14: Windows Node workspace bootstrap (`.npmrc`, jest config) applied once.
    pub workspace_preflight_done: bool,
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
    /// Phase 3b: per-turn kernel event accumulator + projection compare.
    pub kernel_turn_events: KernelTurnEventBuffer,
    /// Phase 3b: turn-end replay coherence + SQLite persist verify.
    pub kernel_turn_replay: KernelTurnReplayVerify,
    /// Phase 3b: per-step v3 effect replay parity.
    pub kernel_v3_step_verify: KernelV3StepVerify,
    /// Resolved `[kernel] machine` (v3 only).
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
    /// Pending IO behind an empty-text v3 `InjectSteer` interpret call.
    pub kernel_pending_inject_steer_kind: Option<super::cycle_briefing_ops::InjectSteerEffectKind>,
    /// When set, `perform_cycle_advance` may emit `CycleAdvanced` for in-turn grants.
    pub kernel_active_cycle_boundary:
        Option<zagens_core::engine::turn_loop::continuation_boundary_policy::OuterBoundaryKind>,
    pub kernel_cycle_advance_ok: Option<bool>,
    /// When true, `RunCompaction` / cycle-advance effects record anchors only (no IO).
    pub kernel_effect_replay_anchor_only: bool,
    /// v3 resume: replace divergent session rows with log-rebuilt preview transcript.
    pub kernel_log_transcript_repair: bool,
    /// v3 resume: persist repaired preview transcript to session store after in-memory repair.
    pub kernel_log_transcript_repair_persist: bool,
    /// Session store handle for repair persist (None disables disk write-back).
    pub session_manager: Option<std::sync::Arc<crate::SessionManager>>,
    /// Per-call v3 approval outcomes stashed before `ExecuteBatch` (cleared each v3 step).
    pub kernel_v3_approval_outcomes:
        std::collections::HashMap<String, super::approval_ops::V3ApprovalStepOutcome>,
    /// Compiler sources explicitly queried via v3 `QueryMemory` this step (batch 8g).
    pub kernel_memory_query_sources: std::collections::BTreeSet<String>,
    /// v3 outer-boundary grant counts for the active turn (batch 5b cont.).
    pub kernel_v3_outer_boundary_grants: V3OuterBoundaryTurnGrants,
    /// Latest Context Explorer assembly report from the compiler path (P2a).
    pub last_context_assembly_report: Option<zagens_core::engine::ContextAssemblyReport>,
}
