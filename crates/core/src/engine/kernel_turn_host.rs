//! Kernel-v3 turn hooks extracted from [`TurnLoopHost`](super::turn_loop::host::TurnLoopHost).
//!
//! Phase 3b batch 6: long-term seam — `TurnLoopHost` shrinks to runtime IO;
//! kernel projection/replay lives here until `EffectInterpreter` owns the loop.

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
        KernelMachineMode::Legacy
    }

    fn kernel_event_sink(&self) -> Option<&KernelEventSink> {
        None
    }

    fn record_kernel_event(&mut self, _event: &KernelEvent) {}

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
}
