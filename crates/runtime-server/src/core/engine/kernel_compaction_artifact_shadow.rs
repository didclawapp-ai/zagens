//! Phase 3b compaction artifact shadow — kernel log vs session-store metadata at resume.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct KernelCompactionArtifactShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelCompactionArtifactShadowStats {
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

static GLOBAL_COMPACTION_ARTIFACT_SHADOW: std::sync::OnceLock<
    std::sync::Arc<KernelCompactionArtifactShadowStats>,
> = std::sync::OnceLock::new();

pub fn register_global_compaction_artifact_shadow_stats(
    stats: std::sync::Arc<KernelCompactionArtifactShadowStats>,
) {
    let _ = GLOBAL_COMPACTION_ARTIFACT_SHADOW.set(stats);
}

/// Record one kernel-vs-session compaction artifact cross-check.
pub fn record_message_compaction_artifact_check(compaction_artifact_ok: bool) {
    let Some(stats) = GLOBAL_COMPACTION_ARTIFACT_SHADOW.get() else {
        return;
    };
    stats.record_comparison();
    if !compaction_artifact_ok {
        stats.record_diff();
    }
}

#[must_use]
pub fn kernel_compaction_artifact_shadow_stats() -> (u64, u64) {
    GLOBAL_COMPACTION_ARTIFACT_SHADOW
        .get()
        .map(|s| s.snapshot())
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_message_compaction_artifact_check_tracks_diffs() {
        let stats = std::sync::Arc::new(KernelCompactionArtifactShadowStats::default());
        register_global_compaction_artifact_shadow_stats(stats.clone());
        record_message_compaction_artifact_check(true);
        record_message_compaction_artifact_check(false);
        assert_eq!(stats.snapshot(), (2, 1));
    }
}
