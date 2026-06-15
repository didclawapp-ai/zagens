//! Phase 3b batch 2 — v3 turn loop observability.

use crate::engine::kernel_mode::KernelMachineMode;

use super::host::TurnLoopHost;

/// Log once per turn when the v3 effect-interpreter path is active.
pub fn log_v3_turn_start<H: TurnLoopHost>(host: &H, turn_id: &str) {
    if host.kernel_machine_mode() == KernelMachineMode::V3 {
        tracing::info!(
            target: "kernel_v3",
            turn_id = %turn_id,
            "v3 turn loop active (CallModel / ExecuteBatch via effect interpreter)"
        );
    }
}
