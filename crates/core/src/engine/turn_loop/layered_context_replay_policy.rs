//! Layered context seam replay coherence (Phase 3b batch 5a / 5c).

use crate::engine::kernel_event::KernelEvent;
use crate::engine::turn_machine::{Effect, TurnKernelProjection, replay_turn_effects};

/// Verify each `LayeredContextSeamInjected` event replays a `RunLayeredContextCheckpoint` effect.
#[must_use]
pub fn verify_layered_context_seam_replay_coherence(events: &[KernelEvent]) -> Option<String> {
    let expected = events
        .iter()
        .filter(|event| matches!(event, KernelEvent::LayeredContextSeamInjected { .. }))
        .count();
    if expected == 0 {
        return None;
    }
    let replay_effects = replay_turn_effects(events)
        .iter()
        .filter(|effect| matches!(effect, Effect::RunLayeredContextCheckpoint))
        .count();
    if replay_effects >= expected {
        return None;
    }
    Some(format!(
        "expected >= {expected} RunLayeredContextCheckpoint replay effects, found {replay_effects}"
    ))
}

/// Verify projection seam counter matches event log.
#[must_use]
pub fn verify_layered_context_seam_projection_coherence(events: &[KernelEvent]) -> Option<String> {
    let projection = TurnKernelProjection::from_events(events);
    let event_count = events
        .iter()
        .filter(|event| matches!(event, KernelEvent::LayeredContextSeamInjected { .. }))
        .count() as u32;
    if projection.layered_context_seam_count == event_count {
        None
    } else {
        Some(format!(
            "layered_context_seam_count proj={} events={event_count}",
            projection.layered_context_seam_count
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::kernel_event::KernelEvent;
    use crate::turn::TurnLoopMode;

    #[test]
    fn seam_replay_coherence_on_fixture_shape() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 5,
            },
            KernelEvent::LayeredContextSeamInjected {
                turn_id: "t1".into(),
                step_idx: 1,
                level: 2,
                messages_covered: 8,
                text_preview: "archived".into(),
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 1,
                request_fp: crate::engine::request_fingerprint::RequestFingerprint {
                    static_prefix_sha256: "a".into(),
                    full_prefix_sha256: "b".into(),
                },
                token_budget: 8192,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: crate::engine::kernel_event::TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        assert!(verify_layered_context_seam_replay_coherence(&events).is_none());
        assert!(verify_layered_context_seam_projection_coherence(&events).is_none());
    }
}
