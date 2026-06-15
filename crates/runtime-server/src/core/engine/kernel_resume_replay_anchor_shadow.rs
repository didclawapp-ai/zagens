//! Phase 3b resume replay anchor shadow — full-thread anchor-only interpret on resume.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct KernelResumeReplayAnchorShadowStats {
    pub resume_runs: AtomicU64,
    pub turns_interpreted: AtomicU64,
    pub anchors_interpreted: AtomicU64,
    pub turns_skipped: AtomicU64,
    pub anchor_alignment_checks: AtomicU64,
    pub anchor_alignment_diffs: AtomicU64,
}

impl KernelResumeReplayAnchorShadowStats {
    pub fn record_run(&self, turns_interpreted: u64, turns_skipped: u64, anchors_interpreted: u64) {
        self.resume_runs.fetch_add(1, Ordering::Relaxed);
        self.turns_interpreted
            .fetch_add(turns_interpreted, Ordering::Relaxed);
        self.turns_skipped
            .fetch_add(turns_skipped, Ordering::Relaxed);
        self.anchors_interpreted
            .fetch_add(anchors_interpreted, Ordering::Relaxed);
    }

    pub fn record_anchor_alignment(&self, anchors_interpreted: u64, expected: u64) {
        self.anchor_alignment_checks.fetch_add(1, Ordering::Relaxed);
        if anchors_interpreted != expected {
            self.anchor_alignment_diffs.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn snapshot(&self) -> (u64, u64, u64, u64) {
        (
            self.resume_runs.load(Ordering::Relaxed),
            self.turns_interpreted.load(Ordering::Relaxed),
            self.anchors_interpreted.load(Ordering::Relaxed),
            self.turns_skipped.load(Ordering::Relaxed),
        )
    }

    pub fn alignment_snapshot(&self) -> (u64, u64) {
        (
            self.anchor_alignment_checks.load(Ordering::Relaxed),
            self.anchor_alignment_diffs.load(Ordering::Relaxed),
        )
    }
}

static GLOBAL_RESUME_REPLAY_ANCHOR_SHADOW: std::sync::OnceLock<
    std::sync::Arc<KernelResumeReplayAnchorShadowStats>,
> = std::sync::OnceLock::new();

pub fn register_global_resume_replay_anchor_shadow_stats(
    stats: std::sync::Arc<KernelResumeReplayAnchorShadowStats>,
) {
    let _ = GLOBAL_RESUME_REPLAY_ANCHOR_SHADOW.set(stats);
}

/// Record one resume replay anchor-only interpret pass (full thread).
pub fn record_resume_replay_anchor_run(
    turns_interpreted: u64,
    turns_skipped: u64,
    anchors_interpreted: u64,
) {
    let Some(stats) = GLOBAL_RESUME_REPLAY_ANCHOR_SHADOW.get() else {
        return;
    };
    stats.record_run(turns_interpreted, turns_skipped, anchors_interpreted);
}

/// Record resume interpret anchor total vs thread `replay_thread_effect_counts` anchor total.
pub fn record_resume_replay_anchor_alignment(anchors_interpreted: u64, expected: u64) {
    let Some(stats) = GLOBAL_RESUME_REPLAY_ANCHOR_SHADOW.get() else {
        return;
    };
    stats.record_anchor_alignment(anchors_interpreted, expected);
}

#[must_use]
pub fn kernel_resume_replay_anchor_shadow_stats() -> (u64, u64, u64, u64) {
    GLOBAL_RESUME_REPLAY_ANCHOR_SHADOW
        .get()
        .map(|s| s.snapshot())
        .unwrap_or((0, 0, 0, 0))
}

#[must_use]
pub fn kernel_resume_replay_anchor_alignment_stats() -> (u64, u64) {
    GLOBAL_RESUME_REPLAY_ANCHOR_SHADOW
        .get()
        .map(|s| s.alignment_snapshot())
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_resume_replay_anchor_run_accumulates() {
        let stats = std::sync::Arc::new(KernelResumeReplayAnchorShadowStats::default());
        register_global_resume_replay_anchor_shadow_stats(stats.clone());
        record_resume_replay_anchor_run(2, 1, 5);
        record_resume_replay_anchor_run(1, 0, 3);
        assert_eq!(stats.snapshot(), (2, 3, 8, 1));
    }

    #[test]
    fn record_resume_replay_anchor_alignment_tracks_diffs() {
        let stats = std::sync::Arc::new(KernelResumeReplayAnchorShadowStats::default());
        register_global_resume_replay_anchor_shadow_stats(stats.clone());
        record_resume_replay_anchor_alignment(4, 4);
        record_resume_replay_anchor_alignment(3, 4);
        assert_eq!(stats.alignment_snapshot(), (2, 1));
    }
}
