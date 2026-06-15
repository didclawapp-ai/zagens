//! Phase 3b request-approval anchor shadow — approval-required plans vs replay at resume.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct KernelRequestApprovalAnchorShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelRequestApprovalAnchorShadowStats {
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

static GLOBAL_REQUEST_APPROVAL_ANCHOR_SHADOW: std::sync::OnceLock<
    std::sync::Arc<KernelRequestApprovalAnchorShadowStats>,
> = std::sync::OnceLock::new();

pub fn register_global_request_approval_anchor_shadow_stats(
    stats: std::sync::Arc<KernelRequestApprovalAnchorShadowStats>,
) {
    let _ = GLOBAL_REQUEST_APPROVAL_ANCHOR_SHADOW.set(stats);
}

/// Record one approval-required step vs `RequestApproval` replay anchor check.
pub fn record_request_approval_anchor_check(request_approval_anchor_ok: bool) {
    let Some(stats) = GLOBAL_REQUEST_APPROVAL_ANCHOR_SHADOW.get() else {
        return;
    };
    stats.record_comparison();
    if !request_approval_anchor_ok {
        stats.record_diff();
    }
}

#[must_use]
pub fn kernel_request_approval_anchor_shadow_stats() -> (u64, u64) {
    GLOBAL_REQUEST_APPROVAL_ANCHOR_SHADOW
        .get()
        .map(|s| s.snapshot())
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_request_approval_anchor_check_tracks_diffs() {
        let stats = std::sync::Arc::new(KernelRequestApprovalAnchorShadowStats::default());
        register_global_request_approval_anchor_shadow_stats(stats.clone());
        record_request_approval_anchor_check(true);
        record_request_approval_anchor_check(false);
        assert_eq!(stats.snapshot(), (2, 1));
    }
}
