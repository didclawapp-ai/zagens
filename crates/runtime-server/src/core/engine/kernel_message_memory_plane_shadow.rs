//! Phase 3b memory-plane user depth shadow — steer/scratchpad vs session text rows.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct KernelMessageMemoryPlaneShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelMessageMemoryPlaneShadowStats {
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

static GLOBAL_MESSAGE_MEMORY_PLANE_SHADOW: std::sync::OnceLock<
    std::sync::Arc<KernelMessageMemoryPlaneShadowStats>,
> = std::sync::OnceLock::new();

pub fn register_global_message_memory_plane_shadow_stats(
    stats: std::sync::Arc<KernelMessageMemoryPlaneShadowStats>,
) {
    let _ = GLOBAL_MESSAGE_MEMORY_PLANE_SHADOW.set(stats);
}

/// Record one session text-user vs kernel memory-plane injection check.
pub fn record_message_memory_plane_check(memory_plane_user_ok: bool) {
    let Some(stats) = GLOBAL_MESSAGE_MEMORY_PLANE_SHADOW.get() else {
        return;
    };
    stats.record_comparison();
    if !memory_plane_user_ok {
        stats.record_diff();
    }
}

#[must_use]
pub fn kernel_message_memory_plane_shadow_stats() -> (u64, u64) {
    GLOBAL_MESSAGE_MEMORY_PLANE_SHADOW
        .get()
        .map(|s| s.snapshot())
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_message_memory_plane_check_tracks_diffs() {
        let stats = std::sync::Arc::new(KernelMessageMemoryPlaneShadowStats::default());
        register_global_message_memory_plane_shadow_stats(stats.clone());
        record_message_memory_plane_check(true);
        record_message_memory_plane_check(false);
        assert_eq!(stats.snapshot(), (2, 1));
    }
}
