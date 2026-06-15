//! Phase 3b projection shadow — compare live host state vs event-log projection.

use std::sync::atomic::{AtomicU64, Ordering};

use tracing::warn;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_machine::{
    LiveTurnSnapshot, TurnKernelProjection, compare_projection_to_live,
};

#[derive(Debug, Default)]
pub struct KernelProjectionShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelProjectionShadowStats {
    pub fn record_comparison(&self) {
        self.comparisons.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_diff(&self) {
        self.diffs.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> (u64, u64) {
        (
            self.comparisons.load(Ordering::Relaxed),
            self.diffs.load(Ordering::Relaxed),
        )
    }
}

/// Per-engine turn accumulator + global stats (for `/v1/runtime/kernel-shadow`).
pub struct KernelProjectionShadow {
    pub stats: std::sync::Arc<KernelProjectionShadowStats>,
    turn_events: Vec<KernelEvent>,
    enabled: bool,
}

impl KernelProjectionShadow {
    pub fn new(enabled: bool) -> Self {
        Self {
            stats: std::sync::Arc::new(KernelProjectionShadowStats::default()),
            turn_events: Vec::new(),
            enabled,
        }
    }

    pub fn reset_turn(&mut self) {
        self.turn_events.clear();
    }

    pub fn record(&mut self, event: KernelEvent) {
        if self.enabled {
            self.turn_events.push(event);
        }
    }

    pub fn turn_events(&self) -> &[KernelEvent] {
        &self.turn_events
    }

    pub fn finish_turn(&mut self, live: &LiveTurnSnapshot) {
        if !self.enabled {
            return;
        }
        self.stats.record_comparison();
        let projection = TurnKernelProjection::from_events(&self.turn_events);
        if let Some(summary) = compare_projection_to_live(live, &projection) {
            self.stats.record_diff();
            warn!(
                target: "kernel_projection_shadow",
                turn_id = %live.turn_id,
                summary,
                "projection shadow diff"
            );
        }
        self.turn_events.clear();
    }
}

static GLOBAL_STATS: std::sync::OnceLock<std::sync::Arc<KernelProjectionShadowStats>> =
    std::sync::OnceLock::new();

pub fn register_global_projection_shadow_stats(stats: std::sync::Arc<KernelProjectionShadowStats>) {
    let _ = GLOBAL_STATS.set(stats);
}

pub fn kernel_projection_shadow_stats() -> (u64, u64) {
    GLOBAL_STATS.get().map(|s| s.snapshot()).unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::turn::TurnLoopMode;

    #[test]
    fn finish_turn_records_diff_when_mismatch() {
        let mut shadow = KernelProjectionShadow::new(true);
        shadow.record(KernelEvent::TurnStarted {
            turn_id: "t1".into(),
            mode: TurnLoopMode::Agent,
            input_text: "hi".into(),
            max_steps: 10,
        });
        let live = LiveTurnSnapshot {
            turn_id: "t1".into(),
            step_idx: 99,
            max_steps: 10,
            ..Default::default()
        };
        shadow.finish_turn(&live);
        let (comparisons, diffs) = shadow.stats.snapshot();
        assert_eq!(comparisons, 1);
        assert_eq!(diffs, 1);
    }
}
