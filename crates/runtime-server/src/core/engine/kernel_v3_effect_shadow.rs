//! Phase 3b v3 step effect shadow — per-step replay parity (`[kernel] machine = "v3"`).

use std::sync::atomic::{AtomicU64, Ordering};

use tracing::warn;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_machine::{
    verify_step_capacity_sleep_anchor, verify_step_compaction_replay_anchor,
    verify_step_continuation_anchor, verify_step_effect_parity,
    verify_step_memory_plane_replay_anchor, verify_step_model_message_anchor,
    verify_step_notify_lsp_anchor, verify_step_request_approval_anchor,
};

#[derive(Debug, Default)]
pub struct KernelV3EffectShadowStats {
    pub comparisons: AtomicU64,
    pub diffs: AtomicU64,
}

impl KernelV3EffectShadowStats {
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

pub struct KernelV3EffectShadow {
    pub stats: std::sync::Arc<KernelV3EffectShadowStats>,
    enabled: bool,
}

impl KernelV3EffectShadow {
    pub fn new(enabled: bool) -> Self {
        Self {
            stats: std::sync::Arc::new(KernelV3EffectShadowStats::default()),
            enabled,
        }
    }

    pub fn verify_step(
        &self,
        turn_events: &[KernelEvent],
        step_idx: u32,
        executed_tool_count: u32,
    ) {
        if !self.enabled {
            return;
        }
        self.stats.record_comparison();
        let mut diffs = Vec::new();
        if let Some(summary) = verify_step_effect_parity(turn_events, step_idx, executed_tool_count)
        {
            diffs.push(summary);
        }
        if let Some(summary) = verify_step_model_message_anchor(turn_events, step_idx) {
            diffs.push(summary);
        }
        if let Some(summary) = verify_step_continuation_anchor(turn_events, step_idx) {
            diffs.push(summary);
        }
        if let Some(summary) = verify_step_notify_lsp_anchor(turn_events, step_idx) {
            diffs.push(summary);
        }
        if let Some(summary) = verify_step_request_approval_anchor(turn_events, step_idx) {
            diffs.push(summary);
        }
        if let Some(summary) = verify_step_memory_plane_replay_anchor(turn_events, step_idx) {
            diffs.push(summary);
        }
        if let Some(summary) =
            zagens_core::engine::turn_loop::memory_plane_query_replay_policy::verify_step_query_memory_anchor(
                turn_events,
                step_idx,
            )
        {
            diffs.push(summary);
        }
        if let Some(summary) = verify_step_compaction_replay_anchor(turn_events, step_idx) {
            diffs.push(summary);
        }
        if let Some(summary) = verify_step_capacity_sleep_anchor(turn_events, step_idx) {
            diffs.push(summary);
        }
        if diffs.is_empty() {
            return;
        }
        self.stats.record_diff();
        warn!(
            target: "kernel_v3_effect_shadow",
            step_idx,
            executed_tool_count,
            summary = diffs.join("; "),
            "v3 step shadow diff"
        );
    }
}

static GLOBAL_V3_EFFECT_SHADOW: std::sync::OnceLock<std::sync::Arc<KernelV3EffectShadowStats>> =
    std::sync::OnceLock::new();

pub fn register_global_v3_effect_shadow_stats(stats: std::sync::Arc<KernelV3EffectShadowStats>) {
    let _ = GLOBAL_V3_EFFECT_SHADOW.set(stats);
}

#[must_use]
pub fn kernel_v3_effect_shadow_stats() -> (u64, u64) {
    GLOBAL_V3_EFFECT_SHADOW
        .get()
        .map(|s| s.snapshot())
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::engine::kernel_event::{KernelEvent, TurnOutcome};
    use zagens_core::turn::TurnLoopMode;

    #[test]
    fn verify_step_passes_minimal_step_log() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 5,
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 1,
                request_fp: zagens_core::engine::request_fingerprint::RequestFingerprint {
                    static_prefix_sha256: "a".into(),
                    full_prefix_sha256: "b".into(),
                },
                token_budget: 8192,
            },
            KernelEvent::ModelMessage {
                turn_id: "t1".into(),
                step_idx: 1,
                usage: zagens_core::models::Usage::default(),
                block_count: 1,
                text_preview: String::new(),
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        let shadow = KernelV3EffectShadow::new(true);
        shadow.verify_step(&events, 1, 0);
        assert_eq!(shadow.stats.snapshot(), (1, 0));
    }
}
