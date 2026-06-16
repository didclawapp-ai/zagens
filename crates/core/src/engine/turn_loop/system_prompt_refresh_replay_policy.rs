//! System prompt refresh replay coherence (Phase 3b batch 5b cont.).

use crate::engine::kernel_event::KernelEvent;
use crate::engine::turn_loop::memory_plane_query_policy::QUERY_USER_MEMORY;
use crate::engine::turn_machine::{Effect, replay_turn_effects};

/// Count refresh-chain `user_memory` queries in the event log (one per refresh cycle).
#[must_use]
pub fn count_refresh_query_cycles(events: &[KernelEvent]) -> u32 {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                KernelEvent::MemoryPlaneQueried { query_key, .. }
                    if query_key == QUERY_USER_MEMORY
            )
        })
        .count() as u32
}

/// Verify logged refresh queries replay a `RefreshSystemPrompt` effect each cycle.
#[must_use]
pub fn verify_system_prompt_refresh_replay_coherence(events: &[KernelEvent]) -> Option<String> {
    let expected = count_refresh_query_cycles(events);
    if expected == 0 {
        return None;
    }
    let replayed = replay_turn_effects(events)
        .iter()
        .filter(|effect| matches!(effect, Effect::RefreshSystemPrompt))
        .count();
    if replayed >= expected as usize {
        return None;
    }
    Some(format!(
        "expected >= {expected} RefreshSystemPrompt replay effects, found {replayed}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::kernel_event::TurnOutcome;
    use crate::engine::request_fingerprint::RequestFingerprint;
    use crate::engine::turn_loop::memory_plane_episodic_policy::QUERY_TOPIC_EPISODIC;
    use crate::turn::TurnLoopMode;

    fn refresh_cycle_events() -> Vec<KernelEvent> {
        vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 5,
            },
            KernelEvent::MemoryPlaneQueried {
                turn_id: "t1".into(),
                step_idx: 1,
                layer: "episodic".into(),
                query_key: QUERY_USER_MEMORY.into(),
                compiler_source: "memory.user".into(),
            },
            KernelEvent::MemoryPlaneQueried {
                turn_id: "t1".into(),
                step_idx: 1,
                layer: "episodic".into(),
                query_key: QUERY_TOPIC_EPISODIC.into(),
                compiler_source: "topic_memory".into(),
            },
            KernelEvent::TopicMemoryInjected {
                turn_id: "t1".into(),
                step_idx: 1,
                block_token_est: 64,
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 1,
                request_fp: RequestFingerprint {
                    static_prefix_sha256: "aa".into(),
                    full_prefix_sha256: "bb".into(),
                },
                token_budget: 8192,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ]
    }

    #[test]
    fn skips_when_no_refresh_queries_logged() {
        let events = vec![KernelEvent::TurnStarted {
            turn_id: "t1".into(),
            mode: TurnLoopMode::Agent,
            input_text: "x".into(),
            max_steps: 3,
        }];
        assert!(verify_system_prompt_refresh_replay_coherence(&events).is_none());
    }

    #[test]
    fn refresh_pair_replays_refresh_system_prompt() {
        let events = refresh_cycle_events();
        assert!(verify_system_prompt_refresh_replay_coherence(&events).is_none());
        let refresh_count = replay_turn_effects(&events)
            .iter()
            .filter(|effect| matches!(effect, Effect::RefreshSystemPrompt))
            .count();
        assert_eq!(refresh_count, 1);
    }

    #[test]
    fn incomplete_refresh_pair_fails_coherence() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 5,
            },
            KernelEvent::MemoryPlaneQueried {
                turn_id: "t1".into(),
                step_idx: 1,
                layer: "episodic".into(),
                query_key: QUERY_USER_MEMORY.into(),
                compiler_source: "memory.user".into(),
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 1,
                request_fp: RequestFingerprint {
                    static_prefix_sha256: "aa".into(),
                    full_prefix_sha256: "bb".into(),
                },
                token_budget: 8192,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        assert!(verify_system_prompt_refresh_replay_coherence(&events).is_some());
    }
}
