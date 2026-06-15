//! Phase 3b effect replay shadow — validate event log drives [`ReplayTurnMachine`].

use std::sync::atomic::{AtomicU64, Ordering};

use tracing::warn;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_machine::verify_effect_replay_chain;

#[derive(Debug, Default)]
pub struct KernelEffectShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelEffectShadowStats {
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

/// Per-engine effect replay verifier (`[kernel] machine = "shadow"`).
pub struct KernelEffectShadow {
    pub stats: std::sync::Arc<KernelEffectShadowStats>,
    enabled: bool,
}

impl KernelEffectShadow {
    pub fn new(enabled: bool) -> Self {
        Self {
            stats: std::sync::Arc::new(KernelEffectShadowStats::default()),
            enabled,
        }
    }

    pub fn verify_turn(&self, events: &[KernelEvent]) {
        if !self.enabled {
            return;
        }
        self.stats.record_comparison();
        if let Some(summary) = verify_effect_replay_chain(events) {
            self.stats.record_diff();
            warn!(
                target: "kernel_effect_shadow",
                summary,
                "effect replay shadow diff"
            );
        }
    }
}

static GLOBAL_STATS: std::sync::OnceLock<std::sync::Arc<KernelEffectShadowStats>> =
    std::sync::OnceLock::new();

pub fn register_global_effect_shadow_stats(stats: std::sync::Arc<KernelEffectShadowStats>) {
    let _ = GLOBAL_STATS.set(stats);
}

pub fn kernel_effect_shadow_stats() -> (u64, u64) {
    GLOBAL_STATS.get().map(|s| s.snapshot()).unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::engine::kernel_event::{PolicyDecision, ToolOutcome, TurnOutcome};
    use zagens_core::engine::request_fingerprint::RequestFingerprint;
    use zagens_core::turn::TurnLoopMode;

    fn sample_events() -> Vec<KernelEvent> {
        vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "hi".into(),
                max_steps: 10,
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 1,
                request_fp: RequestFingerprint {
                    static_prefix_sha256: "a".into(),
                    full_prefix_sha256: "b".into(),
                },
                token_budget: 8192,
            },
            KernelEvent::ToolCallPlanned {
                turn_id: "t1".into(),
                step_idx: 1,
                call_id: "c1".into(),
                tool_name: "read_file".into(),
                input_json: "{}".into(),
                decision: PolicyDecision::new(false, true, true),
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ]
    }

    #[test]
    fn verify_turn_passes_consistent_chain() {
        let shadow = KernelEffectShadow::new(true);
        shadow.verify_turn(&sample_events());
        let (comparisons, diffs) = shadow.stats.snapshot();
        assert_eq!(comparisons, 1);
        assert_eq!(diffs, 0);
    }

    #[test]
    fn verify_turn_records_diff_when_turn_ended_missing() {
        let mut events = sample_events();
        events.retain(|e| !matches!(e, KernelEvent::TurnEnded { .. }));
        let shadow = KernelEffectShadow::new(true);
        shadow.verify_turn(&events);
        let (_, diffs) = shadow.stats.snapshot();
        assert_eq!(diffs, 1);
    }
}
