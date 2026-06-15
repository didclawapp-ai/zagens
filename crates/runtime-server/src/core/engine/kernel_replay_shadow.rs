//! Phase 3b unified replay shadow — log coherence + optional SQLite persist check.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tracing::warn;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_machine::{LiveTurnSnapshot, verify_turn_replay_coherence};
use zagens_runtime_adapters::persist::KernelEventWriter;

#[derive(Debug, Default)]
pub struct KernelReplayShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
    pub persist_diffs: AtomicU64,
}

impl KernelReplayShadowStats {
    pub fn record_comparison(&self) {
        self.comparisons.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_diff(&self) {
        self.diffs.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_persist_diff(&self) {
        self.persist_diffs.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> (u64, u64, u64) {
        (
            self.comparisons.load(Ordering::Relaxed),
            self.diffs.load(Ordering::Relaxed),
            self.persist_diffs.load(Ordering::Relaxed),
        )
    }
}

pub struct KernelReplayShadow {
    pub stats: std::sync::Arc<KernelReplayShadowStats>,
    enabled: bool,
}

impl KernelReplayShadow {
    pub fn new(enabled: bool) -> Self {
        Self {
            stats: std::sync::Arc::new(KernelReplayShadowStats::default()),
            enabled,
        }
    }

    /// In-memory replay gate (sync — called at turn end from `run.rs`).
    pub fn verify_turn_in_memory(&self, events: &[KernelEvent], live: &LiveTurnSnapshot) {
        if !self.enabled {
            return;
        }
        self.stats.record_comparison();
        if let Some(summary) = verify_turn_replay_coherence(events, Some(live)) {
            self.stats.record_diff();
            warn!(
                target: "kernel_replay_shadow",
                turn_id = %live.turn_id,
                summary,
                "turn replay coherence diff"
            );
        }
    }

    /// SQLite round-trip check (async — drain task may lag slightly behind turn end).
    pub async fn verify_turn_persisted(
        &self,
        writer: &KernelEventWriter,
        turn_id: &str,
        in_memory: &[KernelEvent],
    ) {
        if !self.enabled {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Some(summary) = writer.verify_persisted_turn_matches(turn_id, in_memory) {
            self.stats.record_persist_diff();
            warn!(
                target: "kernel_replay_shadow",
                turn_id,
                summary,
                "persisted replay diff"
            );
        }
    }
}

static GLOBAL_STATS: std::sync::OnceLock<std::sync::Arc<KernelReplayShadowStats>> =
    std::sync::OnceLock::new();

pub fn register_global_replay_shadow_stats(stats: std::sync::Arc<KernelReplayShadowStats>) {
    let _ = GLOBAL_STATS.set(stats);
}

pub fn kernel_replay_shadow_stats() -> (u64, u64, u64) {
    GLOBAL_STATS
        .get()
        .map(|s| s.snapshot())
        .unwrap_or((0, 0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::engine::kernel_event::TurnOutcome;
    use zagens_core::turn::TurnLoopMode;

    #[test]
    fn verify_turn_in_memory_passes_minimal_turn() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "hi".into(),
                max_steps: 5,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        let live = LiveTurnSnapshot {
            turn_id: "t1".into(),
            max_steps: 5,
            ..Default::default()
        };
        let shadow = KernelReplayShadow::new(true);
        shadow.verify_turn_in_memory(&events, &live);
        let (_, diffs, _) = shadow.stats.snapshot();
        assert_eq!(diffs, 0);
    }
}
