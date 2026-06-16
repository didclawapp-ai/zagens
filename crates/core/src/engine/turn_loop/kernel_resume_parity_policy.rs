//! Phase 3b batch 5c — kernel log vs session-direct resume parity gate.
//!
//! Counter/timeline/anchor level today; preview body parity when session rows supplied.

use crate::engine::kernel_event::KernelEvent;
use crate::engine::turn_machine::{
    LiveTurnSnapshot, SessionMessageRoleIndex, TurnKernelProjection,
    build_session_message_timeline_coverage, compare_projection_to_live,
    kernel_resume_hints_from_thread_projection, replay_thread_projection,
    verify_resume_anchor_effect_alignment,
};

/// Documented session-direct substrate for a golden resume parity case.
#[derive(Debug, Clone)]
pub struct ResumeLogSessionParityExpectation {
    pub session_message_count: usize,
    pub role_index: Option<SessionMessageRoleIndex>,
}

/// Expected outer-loop counters on the latest turn projection after resume rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumeProjectionCounterExpectation {
    pub step_limit_continuations: u32,
    pub loop_guard_continuations: u32,
    pub cycle_handoff_attempts: u32,
    pub in_turn_cycle_advances: u32,
}

/// Verify latest-turn projection counters match documented live-turn expectations.
#[must_use]
pub fn verify_thread_resume_projection_counter_parity(
    thread_id: &str,
    turn_events: &[(String, Vec<KernelEvent>)],
    expect: ResumeProjectionCounterExpectation,
) -> Option<String> {
    let projection = replay_thread_projection(thread_id, turn_events);
    let proj = &projection.latest_projection;
    let hints = kernel_resume_hints_from_thread_projection(&projection);
    let mut diffs = Vec::new();
    if hints.step_limit_continuations != expect.step_limit_continuations {
        diffs.push(format!(
            "step_limit_continuations hints={} expect={}",
            hints.step_limit_continuations, expect.step_limit_continuations
        ));
    }
    if hints.loop_guard_continuations != expect.loop_guard_continuations {
        diffs.push(format!(
            "loop_guard_continuations hints={} expect={}",
            hints.loop_guard_continuations, expect.loop_guard_continuations
        ));
    }
    if hints.cycle_handoff_attempts != expect.cycle_handoff_attempts {
        diffs.push(format!(
            "cycle_handoff_attempts hints={} expect={}",
            hints.cycle_handoff_attempts, expect.cycle_handoff_attempts
        ));
    }
    if hints.in_turn_cycle_advances != expect.in_turn_cycle_advances {
        diffs.push(format!(
            "in_turn_cycle_advances hints={} expect={}",
            hints.in_turn_cycle_advances, expect.in_turn_cycle_advances
        ));
    }
    if proj.step_limit_continuations != expect.step_limit_continuations {
        diffs.push(format!(
            "step_limit_continuations proj={} expect={}",
            proj.step_limit_continuations, expect.step_limit_continuations
        ));
    }
    if proj.loop_guard_continuations != expect.loop_guard_continuations {
        diffs.push(format!(
            "loop_guard_continuations proj={} expect={}",
            proj.loop_guard_continuations, expect.loop_guard_continuations
        ));
    }
    if proj.cycle_handoff_attempts != expect.cycle_handoff_attempts {
        diffs.push(format!(
            "cycle_handoff_attempts proj={} expect={}",
            proj.cycle_handoff_attempts, expect.cycle_handoff_attempts
        ));
    }
    if proj.in_turn_cycle_advances != expect.in_turn_cycle_advances {
        diffs.push(format!(
            "in_turn_cycle_advances proj={} expect={}",
            proj.in_turn_cycle_advances, expect.in_turn_cycle_advances
        ));
    }
    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("; "))
    }
}

/// Verify a rebuilt log projection matches a sampled live turn snapshot (5c closure helper).
#[must_use]
pub fn verify_turn_log_live_projection_parity(
    events: &[KernelEvent],
    live: &LiveTurnSnapshot,
) -> Option<String> {
    let proj = TurnKernelProjection::from_events(events);
    compare_projection_to_live(live, &proj)
}

/// Verify a thread's kernel log supports the documented session-direct resume substrate.
#[must_use]
pub fn verify_thread_resume_log_session_parity(
    thread_id: &str,
    turn_events: &[(String, Vec<KernelEvent>)],
    expect: &ResumeLogSessionParityExpectation,
    session_messages: Option<&[crate::chat::Message]>,
) -> Option<String> {
    let projection = replay_thread_projection(thread_id, turn_events);
    if !projection.report.all_coherent {
        return Some(format!(
            "thread replay incoherent: {}/{} turns coherent",
            projection.report.turns_coherent, projection.report.turns_with_events
        ));
    }
    let hints = kernel_resume_hints_from_thread_projection(&projection);
    if let Some(summary) = verify_resume_anchor_effect_alignment(
        hints.expected_anchor_effect_count,
        u64::from(projection.effect_counts.anchor_effect_total()),
    ) {
        return Some(summary);
    }
    let coverage = build_session_message_timeline_coverage(
        expect.session_message_count,
        &projection,
        expect.role_index.as_ref(),
        None,
        session_messages,
        Some(turn_events),
    )?;
    if !coverage.overall_ok {
        return coverage.summary.or(Some(
            "resume log/session parity: message timeline coverage failed".into(),
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_projection_counter_parity_passes_lht_fixture() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/lht_continue.json");
        let raw = std::fs::read_to_string(path).expect("fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let turn_id = "golden-lht-001".to_string();
        let turn_events = [(turn_id.clone(), events)];
        assert!(
            verify_thread_resume_projection_counter_parity(
                &turn_id,
                &turn_events,
                ResumeProjectionCounterExpectation {
                    step_limit_continuations: 1,
                    loop_guard_continuations: 1,
                    cycle_handoff_attempts: 0,
                    in_turn_cycle_advances: 0,
                }
            )
            .is_none()
        );
    }
}
