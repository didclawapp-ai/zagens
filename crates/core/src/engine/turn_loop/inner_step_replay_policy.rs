//! Inner step replay coherence — real `ModelRequestIssued` drives `TurnMachine::step`.
//!
//! Post-IO verification rebuilds projection from the turn log prefix before the step's
//! model request, then steps [`ReplayTurnMachine`] on the observed event (not synthetic).

use crate::engine::kernel_event::KernelEvent;
use crate::engine::turn_loop::live_turn_inner_planner::{
    inner_step_baseline_effects, plan_v3_inner_step_baseline,
};
use crate::engine::turn_machine::{
    ReplayTurnMachine, TurnMachine, projection_before_step_model_request, verify_step_effect_parity,
};

/// Count planned tool calls observed in the turn log for one step.
#[must_use]
pub fn executed_tool_count_for_step(turn_events: &[KernelEvent], step_idx: u32) -> u32 {
    turn_events
        .iter()
        .filter(|event| {
            matches!(
                event,
                KernelEvent::ToolCallPlanned { step_idx: s, .. } if *s == step_idx
            )
        })
        .count() as u32
}

/// Verify full step-slice replay (`CallModel` + `ExecuteBatch` + anchors) from the turn log.
#[must_use]
pub fn verify_inner_step_slice_replay_coherence(
    turn_events: &[KernelEvent],
    step_idx: u32,
) -> Option<String> {
    if model_request_issued_for_step(turn_events, step_idx).is_none() {
        return None;
    }
    let executed = executed_tool_count_for_step(turn_events, step_idx);
    verify_step_effect_parity(turn_events, step_idx, executed)
}

/// Locate the observed `ModelRequestIssued` for one step in a turn log.
#[must_use]
pub fn model_request_issued_for_step<'a>(
    turn_events: &'a [KernelEvent],
    step_idx: u32,
) -> Option<&'a KernelEvent> {
    turn_events.iter().find(|event| {
        matches!(
            event,
            KernelEvent::ModelRequestIssued { step_idx: s, .. } if *s == step_idx
        )
    })
}

/// Verify log-driven `ModelRequestIssued` replay matches the inner-step baseline planner.
#[must_use]
pub fn verify_inner_step_model_request_replay_coherence(
    turn_events: &[KernelEvent],
    step_idx: u32,
) -> Option<String> {
    let event = model_request_issued_for_step(turn_events, step_idx)?.clone();
    let KernelEvent::ModelRequestIssued { token_budget, .. } = &event else {
        return None;
    };
    let projection = projection_before_step_model_request(turn_events, step_idx);
    let plan = plan_v3_inner_step_baseline(&projection, *token_budget, None);
    let baseline = inner_step_baseline_effects(&plan);
    let mut replay = ReplayTurnMachine;
    let out = replay.step(&projection, event);
    if out.effects != baseline {
        return Some(format!(
            "step {step_idx} ModelRequestIssued replay mismatch log_effects={:?} planned={baseline:?}",
            out.effects
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::request_fingerprint::RequestFingerprint;
    use crate::engine::turn_loop::memory_plane_query_policy::QUERY_SCRATCHPAD_SUMMARY;
    use crate::engine::turn_machine::Effect;
    use crate::turn::TurnLoopMode;

    fn sample_fp() -> RequestFingerprint {
        RequestFingerprint {
            static_prefix_sha256: "static".into(),
            full_prefix_sha256: "full".into(),
        }
    }

    #[test]
    fn model_request_replay_coherence_accepts_baseline_step() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t".into(),
                mode: TurnLoopMode::Agent,
                input_text: String::new(),
                max_steps: 8,
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t".into(),
                step_idx: 1,
                request_fp: sample_fp(),
                token_budget: 8192,
            },
        ];
        assert!(
            verify_inner_step_model_request_replay_coherence(&events, 1).is_none(),
            "baseline step should replay cleanly"
        );
    }

    #[test]
    fn model_request_replay_uses_prefix_projection_for_query_memory() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t".into(),
                mode: TurnLoopMode::Agent,
                input_text: String::new(),
                max_steps: 8,
            },
            KernelEvent::ScratchpadSummaryInjected {
                turn_id: "t".into(),
                at_step: 0,
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t".into(),
                step_idx: 1,
                request_fp: sample_fp(),
                token_budget: 4096,
            },
        ];
        let projection = projection_before_step_model_request(&events, 1);
        assert!(projection.scratchpad_summary_injected);
        let mut replay = ReplayTurnMachine;
        let event = model_request_issued_for_step(&events, 1).unwrap().clone();
        let out = replay.step(&projection, event);
        assert!(
            out.effects.iter().any(|effect| matches!(
                effect,
                Effect::QueryMemory { query_key, .. } if query_key == QUERY_SCRATCHPAD_SUMMARY
            )),
            "prefix projection should drive QueryMemory before CallModel"
        );
        assert!(
            verify_inner_step_model_request_replay_coherence(&events, 1).is_none(),
            "log-driven replay should match planner when scratchpad summary precedes request"
        );
    }

    #[test]
    fn slice_replay_coherence_accepts_call_model_and_execute_batch() {
        use crate::engine::kernel_event::{PolicyDecision, ToolOutcome, TurnOutcome};
        use crate::models::Usage;

        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t".into(),
                mode: TurnLoopMode::Agent,
                input_text: String::new(),
                max_steps: 8,
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t".into(),
                step_idx: 1,
                request_fp: sample_fp(),
                token_budget: 8192,
            },
            KernelEvent::ModelMessage {
                turn_id: "t".into(),
                step_idx: 1,
                usage: Usage::default(),
                block_count: 1,
                text_preview: String::new(),
                assistant_text: "ok".into(),
            },
            KernelEvent::ToolCallPlanned {
                turn_id: "t".into(),
                step_idx: 1,
                call_id: "c1".into(),
                tool_name: "read_file".into(),
                input_json: "{}".into(),
                decision: PolicyDecision::default(),
            },
            KernelEvent::ToolCallFinished {
                turn_id: "t".into(),
                call_id: "c1".into(),
                tool_name: "read_file".into(),
                outcome: ToolOutcome::Success,
                duration_ms: 1,
                wrote_state: false,
                result_preview: String::new(),
                session_content: String::new(),
            },
            KernelEvent::TurnEnded {
                turn_id: "t".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        assert!(
            verify_inner_step_slice_replay_coherence(&events, 1).is_none(),
            "step slice with one tool should replay cleanly"
        );
    }
}
