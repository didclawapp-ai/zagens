//! Phase 3b continuation anchor shadow — continuation steps vs `InjectSteer` replay at resume.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct KernelContinuationAnchorShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelContinuationAnchorShadowStats {
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

static GLOBAL_CONTINUATION_ANCHOR_SHADOW: std::sync::OnceLock<
    std::sync::Arc<KernelContinuationAnchorShadowStats>,
> = std::sync::OnceLock::new();

pub fn register_global_continuation_anchor_shadow_stats(
    stats: std::sync::Arc<KernelContinuationAnchorShadowStats>,
) {
    let _ = GLOBAL_CONTINUATION_ANCHOR_SHADOW.set(stats);
}

/// Record one continuation-step vs replay-effect anchor check (resume observability).
pub fn record_continuation_anchor_check(continuation_anchor_ok: bool) {
    let Some(stats) = GLOBAL_CONTINUATION_ANCHOR_SHADOW.get() else {
        return;
    };
    stats.record_comparison();
    if !continuation_anchor_ok {
        stats.record_diff();
    }
}

#[must_use]
pub fn kernel_continuation_anchor_shadow_stats() -> (u64, u64) {
    GLOBAL_CONTINUATION_ANCHOR_SHADOW
        .get()
        .map(|s| s.snapshot())
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_continuation_anchor_check_tracks_diffs() {
        let stats = std::sync::Arc::new(KernelContinuationAnchorShadowStats::default());
        register_global_continuation_anchor_shadow_stats(stats.clone());
        record_continuation_anchor_check(true);
        record_continuation_anchor_check(false);
        assert_eq!(stats.snapshot(), (2, 1));
    }
}
