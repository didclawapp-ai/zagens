//! Phase 3b message coverage shadow — session JSON vs kernel log counters at resume.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct KernelMessageCoverageShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelMessageCoverageShadowStats {
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

static GLOBAL_MESSAGE_COVERAGE_SHADOW: std::sync::OnceLock<
    std::sync::Arc<KernelMessageCoverageShadowStats>,
> = std::sync::OnceLock::new();

pub fn register_global_message_coverage_shadow_stats(
    stats: std::sync::Arc<KernelMessageCoverageShadowStats>,
) {
    let _ = GLOBAL_MESSAGE_COVERAGE_SHADOW.set(stats);
}

/// Record one session-vs-kernel message coverage check (resume / thread replay observability).
pub fn record_message_coverage_check(coverage_ok: bool) {
    let Some(stats) = GLOBAL_MESSAGE_COVERAGE_SHADOW.get() else {
        return;
    };
    stats.record_comparison();
    if !coverage_ok {
        stats.record_diff();
    }
}

#[must_use]
pub fn kernel_message_coverage_shadow_stats() -> (u64, u64) {
    GLOBAL_MESSAGE_COVERAGE_SHADOW
        .get()
        .map(|s| s.snapshot())
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_message_coverage_check_tracks_diffs() {
        let stats = std::sync::Arc::new(KernelMessageCoverageShadowStats::default());
        register_global_message_coverage_shadow_stats(stats.clone());
        record_message_coverage_check(true);
        record_message_coverage_check(false);
        assert_eq!(stats.snapshot(), (2, 1));
    }
}
