//! Golden replay fixtures — Phase 3a P3A-6 / Phase 3b 6c.
//!
//! Loads synthetic event logs from `fixtures/harness/kernel-v3-replay/` and
//! verifies deserialize + projection invariants.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::engine::kernel_event::KernelEvent;
    use crate::engine::turn_loop::kernel_resume_parity_policy::{
        ResumeLogSessionParityExpectation, ResumeProjectionCounterExpectation,
        verify_thread_resume_log_session_parity, verify_thread_resume_projection_counter_parity,
        verify_turn_log_live_projection_parity,
    };
    use crate::engine::turn_loop::message_body_rebuild_policy::{
        RebuiltMessageRole, rebuild_preview_messages_from_thread_events,
        verify_log_transcript_rebuild,
    };
    use crate::engine::turn_machine::{
        LiveTurnSnapshot, SessionMessageRoleIndex, TurnKernelProjection,
        build_thread_replay_report, replay_turn_projection, verify_effect_replay_chain,
        verify_guard_projection_chain, verify_memory_projection_chain,
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
        "memory_plane_query.json",
        "resume_thread_parity.json",
        "layered_context_seam.json",
        "message_body_rebuild.json",
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
        assert_eq!(p.cycle_handoff_attempts, 1);
        assert_eq!(p.in_turn_cycle_advances, 1);
        assert!(
            verify_guard_projection_chain(&events).is_none(),
            "guard projection mismatch in cycle_handoff.json"
        );
    }

    #[test]
    fn golden_live_projection_cycle_counter_split() {
        use crate::engine::turn_machine::compare_projection_to_live;

        let events = load_fixture("cycle_handoff.json");
        let proj = TurnKernelProjection::from_events(&events);
        let live = LiveTurnSnapshot {
            turn_id: "golden-cycle-handoff-001".into(),
            step_idx: proj.step_idx,
            max_steps: proj.max_steps,
            cycle_handoff_attempts: 1,
            in_turn_cycle_advances: 1,
            ..Default::default()
        };
        assert!(
            compare_projection_to_live(&live, &proj).is_none(),
            "cycle_handoff.json live/projection counter split mismatch"
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
        assert!(
            crate::engine::turn_loop::memory_plane_projection_policy::verify_memory_plane_layer_coherence(
                &events
            )
            .is_none(),
            "memory plane layer mismatch in scratchpad_compaction.json"
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
        assert!(
            crate::engine::turn_loop::memory_plane_archival_policy::verify_archival_artifact_field_coherence(
                &events
            )
            .is_none(),
            "archival field mismatch in manual_compaction.json"
        );
    }

    #[test]
    fn golden_memory_projection_pure_read_working_layer() {
        let events = load_fixture("pure_read.json");
        assert!(
            crate::engine::turn_loop::memory_plane_working_policy::verify_working_layer_tool_coherence(
                &events
            )
            .is_none(),
            "working layer mismatch in pure_read.json"
        );
        let p = TurnKernelProjection::from_events(&events);
        assert_eq!(p.working_set_path_touch_count, 1);
    }

    #[test]
    fn golden_memory_plane_query_fixture_batch4() {
        let events = load_fixture("memory_plane_query.json");
        assert!(
            crate::engine::turn_loop::memory_plane_wrapup_policy::verify_memory_plane_batch4_coherence(
                &events
            )
            .is_none(),
            "memory_plane_query.json batch4 coherence failed"
        );
        let counts = crate::engine::turn_machine::replay_effect_counts(&events);
        assert_eq!(counts.query_memory, 2);
        assert_eq!(counts.call_model, 2);
        assert!(
            crate::engine::turn_loop::memory_plane_query_replay_policy::verify_step_query_memory_anchor(
                &events, 2
            )
            .is_none()
        );
    }

    #[test]
    fn golden_layered_context_seam_fixture() {
        let events = load_fixture("layered_context_seam.json");
        assert!(
            crate::engine::turn_loop::layered_context_replay_policy::verify_layered_context_seam_replay_coherence(
                &events
            )
            .is_none()
        );
        assert!(
            crate::engine::turn_loop::layered_context_replay_policy::verify_layered_context_seam_projection_coherence(
                &events
            )
            .is_none()
        );
        let counts = crate::engine::turn_machine::replay_effect_counts(&events);
        assert_eq!(counts.call_model, 1);
        assert_eq!(
            events
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        crate::engine::kernel_event::KernelEvent::LayeredContextSeamInjected { .. }
                    )
                })
                .count(),
            1
        );
    }

    #[test]
    fn golden_message_body_rebuild_fixture() {
        let events = load_fixture("message_body_rebuild.json");
        let expected = [
            (RebuiltMessageRole::User, "Fix auth module"),
            (RebuiltMessageRole::User, "check token expiry"),
            (RebuiltMessageRole::Assistant, "I'll inspect auth/token.rs"),
            (
                RebuiltMessageRole::ToolResult,
                "pub fn validate_token() { ... }",
            ),
        ];
        assert!(
            verify_log_transcript_rebuild(&events, &expected).is_none(),
            "message_body_rebuild.json transcript mismatch"
        );
        assert!(
            verify_turn_replay_coherence(&events, None).is_none(),
            "message_body_rebuild.json replay incoherent"
        );
    }

    #[test]
    fn golden_resume_log_session_parity_fixtures() {
        let cases: &[(&str, ResumeLogSessionParityExpectation)] = &[(
            "resume_thread_parity.json",
            ResumeLogSessionParityExpectation {
                session_message_count: 4,
                role_index: Some(SessionMessageRoleIndex {
                    user_message_count: 2,
                    assistant_message_count: 1,
                    tool_result_message_count: 1,
                    text_user_message_count: 2,
                    total_message_count: 4,
                }),
            },
        )];
        for (name, expect) in cases {
            let events = load_fixture(name);
            let transcript_expect = [
                (
                    RebuiltMessageRole::User,
                    "List files in src/ then refactor auth",
                ),
                (RebuiltMessageRole::User, "Check auth module first"),
                (RebuiltMessageRole::Assistant, "I'll list src/ first"),
                (RebuiltMessageRole::ToolResult, "auth.rs\nmain.rs"),
            ];
            assert!(
                verify_log_transcript_rebuild(&events, &transcript_expect).is_none(),
                "resume transcript rebuild failed for {name}"
            );
            let turn_id = events
                .first()
                .and_then(|e| e.turn_id())
                .unwrap_or("unknown")
                .to_string();
            let turn_events = [(turn_id.clone(), events.clone())];
            let preview_messages = rebuild_preview_messages_from_thread_events(&turn_events);
            assert!(
                verify_thread_resume_log_session_parity(
                    &turn_id,
                    &turn_events,
                    expect,
                    Some(&preview_messages),
                )
                .is_none(),
                "resume parity failed for {name}"
            );
        }
    }

    #[test]
    fn golden_resume_projection_counter_parity_fixtures() {
        let cases: &[(&str, ResumeProjectionCounterExpectation)] = &[
            (
                "lht_continue.json",
                ResumeProjectionCounterExpectation {
                    step_limit_continuations: 1,
                    loop_guard_continuations: 1,
                    cycle_handoff_attempts: 0,
                    in_turn_cycle_advances: 0,
                },
            ),
            (
                "cycle_handoff.json",
                ResumeProjectionCounterExpectation {
                    step_limit_continuations: 0,
                    loop_guard_continuations: 0,
                    cycle_handoff_attempts: 1,
                    in_turn_cycle_advances: 1,
                },
            ),
        ];
        for (name, expect) in cases {
            let events = load_fixture(name);
            let turn_id = events
                .first()
                .and_then(|e| e.turn_id())
                .unwrap_or("unknown")
                .to_string();
            let turn_events = [(turn_id.clone(), events.clone())];
            assert!(
                verify_thread_resume_projection_counter_parity(&turn_id, &turn_events, *expect)
                    .is_none(),
                "resume projection counter parity failed for {name}"
            );
            let proj = TurnKernelProjection::from_events(&events);
            let live = LiveTurnSnapshot {
                turn_id,
                step_idx: proj.step_idx,
                max_steps: proj.max_steps,
                step_limit_continuations: expect.step_limit_continuations,
                loop_guard_continuations: expect.loop_guard_continuations,
                cycle_handoff_attempts: expect.cycle_handoff_attempts,
                in_turn_cycle_advances: expect.in_turn_cycle_advances,
                ..Default::default()
            };
            assert!(
                verify_turn_log_live_projection_parity(&events, &live).is_none(),
                "log/live projection parity failed for {name}"
            );
        }
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
