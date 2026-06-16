//! Kernel-v3 turn hooks extracted from the v3 turn-loop host surface.
//!
//! Phase 3b batch 6: kernel projection/replay lives here; runtime IO on
//! [`InnerStepHost`](super::turn_loop::inner_step_host::InnerStepHost) +
//! [`TurnLoopOuterHost`](super::turn_loop::turn_loop_outer_host::TurnLoopOuterHost).

use std::collections::HashSet;

use async_trait::async_trait;

use crate::chat::{LlmClient, Tool};
use crate::engine::kernel_event::KernelEvent;
use crate::engine::kernel_mode::KernelMachineMode;
use crate::engine::loop_guard::LoopGuard;
use crate::engine::turn_loop::v3_step::V3StepOutcome;
use crate::engine::turn_machine::{KernelEventSink, LiveTurnSnapshot};
use crate::turn::{TurnContext, TurnLoopMode};

/// Kernel event double-write + shadow/replay hooks (Phase 3a–3b).
#[async_trait]
pub trait KernelTurnHost {
    /// Tool registry type for runtime v3 interpreter step.
    type V3ToolRegistry: Sync + ?Sized;

    fn kernel_machine_mode(&self) -> KernelMachineMode {
        KernelMachineMode::default()
    }

    fn kernel_event_sink(&self) -> Option<&KernelEventSink> {
        None
    }

    fn record_kernel_event(&mut self, _event: &KernelEvent) {}

    /// v3 outer-loop boundary grant observability (runtime shadow; default no-op).
    fn record_v3_outer_boundary_grant(
        &mut self,
        _kind: crate::engine::turn_loop::continuation_boundary_policy::OuterBoundaryKind,
    ) {
    }

    fn reset_kernel_projection_shadow(&mut self) {}

    fn kernel_shadow_turn_events(&self) -> Vec<KernelEvent> {
        Vec::new()
    }

    fn finish_kernel_projection_shadow(&mut self, _live: &LiveTurnSnapshot) {}

    /// Turn-end kernel shadow pipeline (projection + replay shadows; runtime overrides).
    async fn finish_kernel_turn_shadow(&mut self, live: &LiveTurnSnapshot) {
        self.finish_kernel_projection_shadow(live);
    }

    fn sync_kernel_turn_frame(&mut self, _turn: &TurnContext) {}

    /// Apply persisted projection hints when a thread engine is loaded (Phase 3b 6e).
    fn apply_kernel_resume_hints(
        &mut self,
        _hints: &crate::engine::turn_machine::KernelResumeHints,
    ) {
    }

    /// Optional v3 turn step via runtime [`EffectInterpreter`] (default: `None` → core fallback).
    #[allow(clippy::too_many_arguments)]
    async fn try_run_v3_turn_step(
        &mut self,
        _turn: &mut TurnContext,
        _client: &dyn LlmClient,
        _mode: TurnLoopMode,
        _tool_catalog: &mut [Tool],
        _active_tool_names: &mut HashSet<String>,
        _force_update_plan_first: bool,
        _stream_retry_attempts: &mut u32,
        _context_recovery_attempts: &mut u8,
        _length_continuations: &mut u32,
        _turn_error: &mut Option<String>,
        _loop_guard: &mut LoopGuard,
        _consecutive_tool_error_steps: u32,
        _tool_registry: Option<&Self::V3ToolRegistry>,
    ) -> Option<V3StepOutcome> {
        None
    }

    /// Optional v3 pre-inner-step baseline via runtime [`EffectInterpreter`]
    /// (`RunCompaction` + `RunLayeredContextCheckpoint` plan). Default: `false` →
    /// [`super::turn_loop::TurnLoopOuterHost::run_pre_inner_step_auto_compaction`] +
    /// [`super::turn_loop::TurnLoopOuterHost::run_pre_inner_step_layered_context`].
    async fn try_run_pre_inner_step_baseline(
        &mut self,
        _client: &dyn LlmClient,
        _turn: &TurnContext,
    ) -> bool {
        false
    }

    /// Optional v3 system-prompt refresh queries via runtime [`EffectInterpreter`]
    /// (`QueryMemory` plan from [`system_prompt_refresh_policy`]). Default: `false`.
    async fn try_run_system_prompt_refresh_queries(&mut self, _turn: &TurnContext) -> bool {
        false
    }

    /// Optional v3 system-prompt refresh (QueryMemory chain + assembly via runtime ops).
    /// When `true`, [`super::turn_loop::TurnLoopOuterHost::refresh_system_prompt`] is skipped.
    async fn try_run_system_prompt_refresh(
        &mut self,
        _turn: &TurnContext,
        _mode: TurnLoopMode,
    ) -> bool {
        false
    }
}
