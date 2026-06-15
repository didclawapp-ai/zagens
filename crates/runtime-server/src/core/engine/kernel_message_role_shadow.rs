//! Phase 3b message role-index shadow — session role counts vs kernel log at resume.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct KernelMessageRoleShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelMessageRoleShadowStats {
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

static GLOBAL_MESSAGE_ROLE_SHADOW: std::sync::OnceLock<
    std::sync::Arc<KernelMessageRoleShadowStats>,
> = std::sync::OnceLock::new();

pub fn register_global_message_role_shadow_stats(
    stats: std::sync::Arc<KernelMessageRoleShadowStats>,
) {
    let _ = GLOBAL_MESSAGE_ROLE_SHADOW.set(stats);
}

/// Record one session role-index vs kernel log check (resume observability).
pub fn record_message_role_index_check(role_index_ok: bool) {
    let Some(stats) = GLOBAL_MESSAGE_ROLE_SHADOW.get() else {
        return;
    };
    stats.record_comparison();
    if !role_index_ok {
        stats.record_diff();
    }
}

#[must_use]
pub fn kernel_message_role_shadow_stats() -> (u64, u64) {
    GLOBAL_MESSAGE_ROLE_SHADOW
        .get()
        .map(|s| s.snapshot())
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_message_role_index_check_tracks_diffs() {
        let stats = std::sync::Arc::new(KernelMessageRoleShadowStats::default());
        register_global_message_role_shadow_stats(stats.clone());
        record_message_role_index_check(true);
        record_message_role_index_check(false);
        assert_eq!(stats.snapshot(), (2, 1));
    }
}
