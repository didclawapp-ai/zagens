//! Golden replay fixtures — Phase 3a P3A-6 / Phase 3b 6c.
//!
//! Loads synthetic event logs from `fixtures/harness/kernel-v3-replay/` and
//! verifies deserialize + projection invariants.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::engine::kernel_event::KernelEvent;
    use crate::engine::turn_machine::{
        TurnKernelProjection, build_thread_replay_report, replay_turn_projection,
        verify_effect_replay_chain, verify_guard_projection_chain, verify_memory_projection_chain,
        verify_turn_replay_coherence,
    };

    const ALL_FIXTURES: &[&str] = &[
        "pure_read.json",
        "write_batch.json",
        "lht_continue.json",
        "loop_guard.json",
        "scratchpad_compaction.json",
        "cycle_handoff.json",
        "overflow_recovery.json",
        "capacity_checkpoint.json",
        "manual_compaction.json",
        "deferred_activation.json",
    ];

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay")
            .join(name)
    }

    fn load_fixture(name: &str) -> Vec<KernelEvent> {
        let path = fixture_path(name);
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
        serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("parse fixture {}: {e}", path.display()))
    }

    #[test]
    fn golden_replay_pure_read() {
        let events = load_fixture("pure_read.json");
        assert_eq!(events.len(), 7);
        let p = TurnKernelProjection::from_events(&events);
        assert_eq!(p.turn_id, "golden-pure-read-001");
        assert_eq!(p.readonly_tool_successes, 1);
        assert!(p.outcome.is_some());
    }

    #[test]
    fn golden_replay_write_batch() {
        let events = load_fixture("write_batch.json");
        let p = TurnKernelProjection::from_events(&events);
        assert!(
            p.active_tool_names.contains("tool_search_tool_regex"),
            "deferred tool must appear in projection"
        );
    }

    #[test]
    fn golden_replay_lht_continue() {
        let events = load_fixture("lht_continue.json");
        let p = TurnKernelProjection::from_events(&events);
        assert_eq!(p.step_limit_continuations, 1);
        assert_eq!(p.loop_guard_continuations, 1);
        assert_eq!(p.steer_injection_count, 1);
    }

    #[test]
    fn golden_loop_guard_replay_coherence() {
        let events = load_fixture("loop_guard.json");
        assert!(
            crate::engine::turn_loop::loop_guard_replay_policy::verify_loop_guard_replay_coherence(
                &events
            )
            .is_none(),
            "loop guard replay mismatch in loop_guard.json"
        );
    }

    #[test]
    fn golden_guard_projection_loop_guard() {
        let events = load_fixture("loop_guard.json");
        let p = TurnKernelProjection::from_events(&events);
        assert_eq!(p.loop_guard_triggered_count, 1);
        assert_eq!(p.loop_guard_continuations, 1);
        assert!(
            verify_guard_projection_chain(&events).is_none(),
            "guard projection mismatch in loop_guard.json"
        );
    }

    #[test]
    fn golden_guard_projection_lht_continue() {
        let events = load_fixture("lht_continue.json");
        assert!(
            verify_guard_projection_chain(&events).is_none(),
            "guard projection mismatch in lht_continue.json"
        );
    }

    #[test]
    fn golden_guard_projection_cycle_handoff() {
        let events = load_fixture("cycle_handoff.json");
        let p = TurnKernelProjection::from_events(&events);
        assert_eq!(p.cycle_handoff_attempts, 2);
        assert!(
            verify_guard_projection_chain(&events).is_none(),
            "guard projection mismatch in cycle_handoff.json"
        );
    }

    #[test]
    fn golden_capacity_replay_coherence() {
        let events = load_fixture("capacity_checkpoint.json");
        assert!(
            crate::engine::turn_loop::capacity_replay_policy::verify_capacity_checkpoint_field_coherence(
                &events
            )
            .is_none(),
            "capacity field coherence mismatch in capacity_checkpoint.json"
        );
        assert!(
            crate::engine::turn_machine::verify_capacity_effect_replay_coherence(&events).is_none(),
            "capacity effect replay mismatch in capacity_checkpoint.json"
        );
    }

    #[test]
    fn golden_capacity_checkpoint_projection() {
        let events = load_fixture("capacity_checkpoint.json");
        let p = TurnKernelProjection::from_events(&events);
        assert_eq!(p.capacity_checkpoint_count, 2);
        assert_eq!(
            p.last_capacity_action,
            Some(crate::engine::kernel_event::CapacityAction::Trim)
        );
        assert!(
            verify_guard_projection_chain(&events).is_none(),
            "guard projection mismatch in capacity_checkpoint.json"
        );
    }

    #[test]
    fn golden_effect_replay_chain_all_fixtures() {
        for name in ALL_FIXTURES {
            let events = load_fixture(name);
            assert!(
                verify_effect_replay_chain(&events).is_none(),
                "effect replay mismatch in {name}"
            );
        }
    }

    #[test]
    fn golden_memory_projection_scratchpad_compaction() {
        let events = load_fixture("scratchpad_compaction.json");
        let p = TurnKernelProjection::from_events(&events);
        assert!(p.scratchpad_summary_injected);
        assert_eq!(p.scratchpad_reminder_count, 1);
        assert_eq!(p.compaction_artifact_count, 1);
        assert_eq!(p.cycle_briefing_count, 1);
        assert!(
            verify_memory_projection_chain(&events).is_none(),
            "memory projection mismatch in scratchpad_compaction.json"
        );
    }

    #[test]
    fn golden_memory_projection_manual_compaction() {
        let events = load_fixture("manual_compaction.json");
        let p = TurnKernelProjection::from_events(&events);
        assert_eq!(p.compaction_artifact_count, 1);
        assert!(
            verify_memory_projection_chain(&events).is_none(),
            "memory projection mismatch in manual_compaction.json"
        );
    }

    #[test]
    fn golden_replay_coherence_all_fixtures() {
        for name in ALL_FIXTURES {
            let events = load_fixture(name);
            let report = replay_turn_projection(&events);
            assert!(
                report.outcome.is_some(),
                "fixture {name} should end with TurnEnded outcome"
            );
            assert!(
                verify_turn_replay_coherence(&events, None).is_none(),
                "replay coherence mismatch in {name}"
            );
        }
    }

    #[test]
    fn golden_replay_all_fixtures_round_trip() {
        for name in ALL_FIXTURES {
            let events = load_fixture(name);
            let json = serde_json::to_string(&events).expect("serialize");
            let back: Vec<KernelEvent> = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back.len(), events.len(), "round-trip length for {name}");
        }
    }

    #[test]
    fn golden_thread_replay_report_aggregates_fixtures() {
        let pairs: Vec<(String, Vec<KernelEvent>)> = ALL_FIXTURES
            .iter()
            .map(|name| {
                let events = load_fixture(name);
                let turn_id = events
                    .first()
                    .and_then(|e| e.turn_id())
                    .unwrap_or("unknown")
                    .to_string();
                (turn_id, events)
            })
            .collect();
        let report = build_thread_replay_report("golden-thread", &pairs);
        assert_eq!(report.turn_count, ALL_FIXTURES.len());
        assert_eq!(report.turns_with_events, ALL_FIXTURES.len());
        assert!(report.all_coherent);
    }
}
