//! Phase 3b message timeline shadow — log anchor coherence at replay/resume.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct KernelMessageTimelineShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelMessageTimelineShadowStats {
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

static GLOBAL_MESSAGE_TIMELINE_SHADOW: std::sync::OnceLock<
    std::sync::Arc<KernelMessageTimelineShadowStats>,
> = std::sync::OnceLock::new();

pub fn register_global_message_timeline_shadow_stats(
    stats: std::sync::Arc<KernelMessageTimelineShadowStats>,
) {
    let _ = GLOBAL_MESSAGE_TIMELINE_SHADOW.set(stats);
}

/// Record one timeline coherence check (thread replay / resume / v3 turn end).
pub fn record_timeline_coherence_check(coherence_ok: bool) {
    let Some(stats) = GLOBAL_MESSAGE_TIMELINE_SHADOW.get() else {
        return;
    };
    stats.record_comparison();
    if !coherence_ok {
        stats.record_diff();
    }
}

#[must_use]
pub fn kernel_message_timeline_shadow_stats() -> (u64, u64) {
    GLOBAL_MESSAGE_TIMELINE_SHADOW
        .get()
        .map(|s| s.snapshot())
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_timeline_coherence_check_tracks_diffs() {
        let stats = std::sync::Arc::new(KernelMessageTimelineShadowStats::default());
        register_global_message_timeline_shadow_stats(stats.clone());
        record_timeline_coherence_check(true);
        record_timeline_coherence_check(false);
        assert_eq!(stats.snapshot(), (2, 1));
    }
}
