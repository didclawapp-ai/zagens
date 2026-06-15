//! Phase 3b compaction replay anchor shadow — artifact/trim events vs `RunCompaction` replay at resume.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct KernelCompactionReplayAnchorShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelCompactionReplayAnchorShadowStats {
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

static GLOBAL_COMPACTION_REPLAY_ANCHOR_SHADOW: std::sync::OnceLock<
    std::sync::Arc<KernelCompactionReplayAnchorShadowStats>,
> = std::sync::OnceLock::new();

pub fn register_global_compaction_replay_anchor_shadow_stats(
    stats: std::sync::Arc<KernelCompactionReplayAnchorShadowStats>,
) {
    let _ = GLOBAL_COMPACTION_REPLAY_ANCHOR_SHADOW.set(stats);
}

/// Record one compaction-event vs replay-effect anchor check.
pub fn record_compaction_replay_anchor_check(compaction_replay_anchor_ok: bool) {
    let Some(stats) = GLOBAL_COMPACTION_REPLAY_ANCHOR_SHADOW.get() else {
        return;
    };
    stats.record_comparison();
    if !compaction_replay_anchor_ok {
        stats.record_diff();
    }
}

#[must_use]
pub fn kernel_compaction_replay_anchor_shadow_stats() -> (u64, u64) {
    GLOBAL_COMPACTION_REPLAY_ANCHOR_SHADOW
        .get()
        .map(|s| s.snapshot())
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_compaction_replay_anchor_check_tracks_diffs() {
        let stats = std::sync::Arc::new(KernelCompactionReplayAnchorShadowStats::default());
        register_global_compaction_replay_anchor_shadow_stats(stats.clone());
        record_compaction_replay_anchor_check(true);
        record_compaction_replay_anchor_check(false);
        assert_eq!(stats.snapshot(), (2, 1));
    }
}
