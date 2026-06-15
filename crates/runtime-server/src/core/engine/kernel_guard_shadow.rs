//! Phase 3b guard projection shadow — continuation/capacity counter sanity.

use std::sync::atomic::{AtomicU64, Ordering};

use tracing::warn;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_machine::verify_guard_projection_chain;

#[derive(Debug, Default)]
pub struct KernelGuardShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelGuardShadowStats {
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

pub struct KernelGuardShadow {
    pub stats: std::sync::Arc<KernelGuardShadowStats>,
    enabled: bool,
}

impl KernelGuardShadow {
    pub fn new(enabled: bool) -> Self {
        Self {
            stats: std::sync::Arc::new(KernelGuardShadowStats::default()),
            enabled,
        }
    }

    pub fn verify_turn(&self, events: &[KernelEvent]) {
        if !self.enabled {
            return;
        }
        self.stats.record_comparison();
        if let Some(summary) = verify_guard_projection_chain(events) {
            self.stats.record_diff();
            warn!(
                target: "kernel_guard_shadow",
                summary,
                "guard projection shadow diff"
            );
        }
    }
}

static GLOBAL_STATS: std::sync::OnceLock<std::sync::Arc<KernelGuardShadowStats>> =
    std::sync::OnceLock::new();

pub fn register_global_guard_shadow_stats(stats: std::sync::Arc<KernelGuardShadowStats>) {
    let _ = GLOBAL_STATS.set(stats);
}

pub fn kernel_guard_shadow_stats() -> (u64, u64) {
    GLOBAL_STATS.get().map(|s| s.snapshot()).unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::engine::kernel_event::TurnOutcome;
    use zagens_core::turn::TurnLoopMode;

    #[test]
    fn verify_turn_passes_lht_fixture_shape() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 20,
            },
            KernelEvent::StepLimitContinuation {
                turn_id: "t1".into(),
                step_idx: 20,
                lht_objective_injected: true,
            },
            KernelEvent::LoopGuardContinuation {
                turn_id: "t1".into(),
                step_idx: 22,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 23,
            },
        ];
        let shadow = KernelGuardShadow::new(true);
        shadow.verify_turn(&events);
        let (_, diffs) = shadow.stats.snapshot();
        assert_eq!(diffs, 0);
    }
}
