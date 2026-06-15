//! v3 capacity cooldown back-off — routes [`Effect::Sleep`] through the interpreter.

use std::time::Duration;

use super::*;

impl Engine {
    /// Sleep for `millis` (honours cancel token; no-op in replay anchor-only mode).
    pub(in crate::core::engine) async fn run_sleep_effect(&mut self, millis: u64) {
        if self.effect_replay_anchor_only() {
            tracing::info!(
                target: "kernel_v3",
                millis,
                "replay anchor-only: skipping Sleep IO"
            );
            return;
        }
        if millis == 0 {
            return;
        }
        tracing::info!(
            target: "kernel_v3",
            millis,
            "v3 effect: Sleep (capacity back-off)"
        );
        tokio::select! {
            _ = self.0.cancel_token.cancelled() => {}
            _ = tokio::time::sleep(Duration::from_millis(millis)) => {}
        }
    }
}
