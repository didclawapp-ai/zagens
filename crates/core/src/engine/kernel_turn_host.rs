//! Kernel-v3 turn hooks extracted from [`TurnLoopHost`](super::turn_loop::host::TurnLoopHost).
//!
//! Phase 3b batch 6: long-term seam — `TurnLoopHost` shrinks to runtime IO;
//! kernel projection/replay lives here until `EffectInterpreter` owns the loop.

use crate::engine::kernel_event::KernelEvent;
use crate::engine::kernel_mode::KernelMachineMode;
use crate::engine::turn_machine::{KernelEventSink, LiveTurnSnapshot};
use crate::turn::TurnContext;

/// Kernel event double-write + shadow/replay hooks (Phase 3a–3b).
pub trait KernelTurnHost {
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

    fn sync_kernel_turn_frame(&mut self, _turn: &TurnContext) {}
}
