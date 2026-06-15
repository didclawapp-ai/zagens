//! Phase 3b memory-plane projection shadow — scratchpad / compaction / briefing counters.

use std::sync::atomic::{AtomicU64, Ordering};

use tracing::warn;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_machine::verify_memory_projection_chain;

#[derive(Debug, Default)]
pub struct KernelMemoryShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelMemoryShadowStats {
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

pub struct KernelMemoryShadow {
    pub stats: std::sync::Arc<KernelMemoryShadowStats>,
    enabled: bool,
}

impl KernelMemoryShadow {
    pub fn new(enabled: bool) -> Self {
        Self {
            stats: std::sync::Arc::new(KernelMemoryShadowStats::default()),
            enabled,
        }
    }

    pub fn verify_turn(&self, events: &[KernelEvent]) {
        if !self.enabled {
            return;
        }
        self.stats.record_comparison();
        if let Some(summary) = verify_memory_projection_chain(events) {
            self.stats.record_diff();
            warn!(
                target: "kernel_memory_shadow",
                summary,
                "memory projection shadow diff"
            );
        }
    }
}

static GLOBAL_STATS: std::sync::OnceLock<std::sync::Arc<KernelMemoryShadowStats>> =
    std::sync::OnceLock::new();

pub fn register_global_memory_shadow_stats(stats: std::sync::Arc<KernelMemoryShadowStats>) {
    let _ = GLOBAL_STATS.set(stats);
}

pub fn kernel_memory_shadow_stats() -> (u64, u64) {
    GLOBAL_STATS.get().map(|s| s.snapshot()).unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::engine::kernel_event::MessageRange;

    #[test]
    fn verify_turn_passes_memory_fixture_shape() {
        let events = vec![
            KernelEvent::ScratchpadSummaryInjected {
                turn_id: "t1".into(),
                at_step: 2,
            },
            KernelEvent::ScratchpadReminderInjected {
                turn_id: "t1".into(),
                step_idx: 2,
                area_path: "src/main.rs".into(),
            },
            KernelEvent::CompactionArtifactCreated {
                turn_id: "t1".into(),
                artifact_id: "art-1".into(),
                replaced_range: MessageRange { from: 1, to: 5 },
                summary_token_count: 120,
            },
        ];
        let shadow = KernelMemoryShadow::new(true);
        shadow.verify_turn(&events);
        let (_, diffs) = shadow.stats.snapshot();
        assert_eq!(diffs, 0);
    }
}
