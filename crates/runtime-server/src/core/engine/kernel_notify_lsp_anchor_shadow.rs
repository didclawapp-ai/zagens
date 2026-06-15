//! Phase 3b notify-LSP anchor shadow — edit-tool steps vs `NotifyLsp` replay at resume.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct KernelNotifyLspAnchorShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelNotifyLspAnchorShadowStats {
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

static GLOBAL_NOTIFY_LSP_ANCHOR_SHADOW: std::sync::OnceLock<
    std::sync::Arc<KernelNotifyLspAnchorShadowStats>,
> = std::sync::OnceLock::new();

pub fn register_global_notify_lsp_anchor_shadow_stats(
    stats: std::sync::Arc<KernelNotifyLspAnchorShadowStats>,
) {
    let _ = GLOBAL_NOTIFY_LSP_ANCHOR_SHADOW.set(stats);
}

/// Record one edit-tool step vs `NotifyLsp` replay anchor check.
pub fn record_notify_lsp_anchor_check(notify_lsp_anchor_ok: bool) {
    let Some(stats) = GLOBAL_NOTIFY_LSP_ANCHOR_SHADOW.get() else {
        return;
    };
    stats.record_comparison();
    if !notify_lsp_anchor_ok {
        stats.record_diff();
    }
}

#[must_use]
pub fn kernel_notify_lsp_anchor_shadow_stats() -> (u64, u64) {
    GLOBAL_NOTIFY_LSP_ANCHOR_SHADOW
        .get()
        .map(|s| s.snapshot())
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_notify_lsp_anchor_check_tracks_diffs() {
        let stats = std::sync::Arc::new(KernelNotifyLspAnchorShadowStats::default());
        register_global_notify_lsp_anchor_shadow_stats(stats.clone());
        record_notify_lsp_anchor_check(true);
        record_notify_lsp_anchor_check(false);
        assert_eq!(stats.snapshot(), (2, 1));
    }
}
