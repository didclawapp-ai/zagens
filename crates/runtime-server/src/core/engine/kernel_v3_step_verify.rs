//! v3 per-step effect replay parity checks.

use tracing::warn;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_machine::{
    verify_step_capacity_sleep_anchor, verify_step_compaction_replay_anchor,
    verify_step_continuation_anchor, verify_step_effect_parity,
    verify_step_memory_plane_replay_anchor, verify_step_model_message_anchor,
    verify_step_notify_lsp_anchor, verify_step_request_approval_anchor,
};

pub struct KernelV3StepVerify {
    enabled: bool,
}

impl KernelV3StepVerify {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
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
        warn!(
            target: "kernel_v3_step_verify",
            step_idx,
            executed_tool_count,
            summary = diffs.join("; "),
            "v3 step replay diff"
        );
    }
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
                assistant_text: String::new(),
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        let verify = KernelV3StepVerify::new(true);
        verify.verify_step(&events, 1, 0);
    }
}
