//! Phase 3b compaction depth shadow — session + removed rows vs kernel plane estimate.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct KernelMessageCompactionShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelMessageCompactionShadowStats {
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

static GLOBAL_MESSAGE_COMPACTION_SHADOW: std::sync::OnceLock<
    std::sync::Arc<KernelMessageCompactionShadowStats>,
> = std::sync::OnceLock::new();

pub fn register_global_message_compaction_shadow_stats(
    stats: std::sync::Arc<KernelMessageCompactionShadowStats>,
) {
    let _ = GLOBAL_MESSAGE_COMPACTION_SHADOW.set(stats);
}

/// Record one compaction depth check at resume / thread replay.
pub fn record_message_compaction_depth_check(compaction_depth_ok: bool) {
    let Some(stats) = GLOBAL_MESSAGE_COMPACTION_SHADOW.get() else {
        return;
    };
    stats.record_comparison();
    if !compaction_depth_ok {
        stats.record_diff();
    }
}

#[must_use]
pub fn kernel_message_compaction_shadow_stats() -> (u64, u64) {
    GLOBAL_MESSAGE_COMPACTION_SHADOW
        .get()
        .map(|s| s.snapshot())
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_message_compaction_depth_check_tracks_diffs() {
        let stats = std::sync::Arc::new(KernelMessageCompactionShadowStats::default());
        register_global_message_compaction_shadow_stats(stats.clone());
        record_message_compaction_depth_check(true);
        record_message_compaction_depth_check(false);
        assert_eq!(stats.snapshot(), (2, 1));
    }
}
