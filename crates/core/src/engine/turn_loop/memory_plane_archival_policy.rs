//! Memory Plane archival layer — compaction artifact field + session cross-check (8b).

use crate::engine::kernel_event::KernelEvent;
use crate::engine::turn_machine::{
    SessionCompactionArtifactEntry, ThreadCompactionReplayEntry, TurnKernelProjection,
    compaction_messages_removed_count, replay_thread_compaction_timeline,
    verify_compaction_artifacts_vs_kernel_timeline,
};

use super::memory_plane_projection_policy::count_memory_plane_layers;

/// Rebuild archival anchors for a single turn log.
#[must_use]
pub fn replay_turn_archival_timeline(events: &[KernelEvent]) -> Vec<ThreadCompactionReplayEntry> {
    let turn_id = events
        .iter()
        .find_map(|event| match event {
            KernelEvent::TurnStarted { turn_id, .. } => Some(turn_id.clone()),
            KernelEvent::CompactionArtifactCreated { turn_id, .. } => Some(turn_id.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "unknown".to_string());
    replay_thread_compaction_timeline(&[(turn_id, events.to_vec())])
}

/// Verify `CompactionArtifactCreated` rows are internally consistent and align with projection.
#[must_use]
pub fn verify_archival_artifact_field_coherence(events: &[KernelEvent]) -> Option<String> {
    let timeline = replay_turn_archival_timeline(events);
    if timeline.is_empty() {
        return None;
    }

    let mut issues = Vec::new();
    for (idx, event) in events.iter().enumerate() {
        let KernelEvent::CompactionArtifactCreated {
            artifact_id,
            replaced_range,
            summary_token_count,
            ..
        } = event
        else {
            continue;
        };

        if artifact_id.trim().is_empty() {
            issues.push(format!("artifact[{idx}] empty artifact_id"));
        }
        if replaced_range.from > replaced_range.to {
            issues.push(format!(
                "artifact[{idx}] invalid range {}..{}",
                replaced_range.from, replaced_range.to
            ));
        }
        if *summary_token_count == 0 {
            issues.push(format!("artifact[{idx}] summary_token_count is zero"));
        }
        let expected_removed =
            compaction_messages_removed_count(replaced_range.from, replaced_range.to);
        if expected_removed == 0 && replaced_range.from <= replaced_range.to {
            issues.push(format!(
                "artifact[{idx}] replaced_range {}..{} removes zero messages",
                replaced_range.from, replaced_range.to
            ));
        }
    }

    let projection = TurnKernelProjection::from_events(events);
    let archival_layer = count_memory_plane_layers(events).archival;
    let timeline_len = timeline.len() as u32;
    if projection.compaction_artifact_count != timeline_len {
        issues.push(format!(
            "compaction_artifact_count proj={} timeline={timeline_len}",
            projection.compaction_artifact_count
        ));
    }
    if archival_layer != timeline_len {
        issues.push(format!(
            "archival layer={archival_layer} timeline={timeline_len}"
        ));
    }

    for entry in &timeline {
        let expected = compaction_messages_removed_count(entry.replaced_from, entry.replaced_to);
        if entry.messages_removed_count != expected {
            issues.push(format!(
                "artifact {} removed_count entry={} expected={expected}",
                entry.artifact_id, entry.messages_removed_count
            ));
        }
    }

    if issues.is_empty() {
        None
    } else {
        Some(issues.join("; "))
    }
}

/// Cross-check kernel archival anchors against session-store compaction rows (same turn/thread slice).
#[must_use]
pub fn verify_archival_layer_vs_session(
    events: &[KernelEvent],
    session: &[SessionCompactionArtifactEntry],
) -> Option<String> {
    let timeline = replay_turn_archival_timeline(events);
    if timeline.is_empty() && session.is_empty() {
        return None;
    }
    if let Some(summary) = verify_archival_artifact_field_coherence(events) {
        return Some(summary);
    }
    verify_compaction_artifacts_vs_kernel_timeline(&timeline, session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::kernel_event::{MessageRange, TurnOutcome};
    use crate::turn::TurnLoopMode;

    #[test]
    fn manual_compaction_fixture_archival_coherence_passes() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/manual_compaction.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        assert!(verify_archival_artifact_field_coherence(&events).is_none());
        let timeline = replay_turn_archival_timeline(&events);
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].artifact_id, "art-manual-001");
        assert_eq!(timeline[0].messages_removed_count, 16);
        let session = vec![SessionCompactionArtifactEntry {
            artifact_id: "art-manual-001".into(),
            replaced_start: 4,
            replaced_end: 20,
            messages_removed_count: 16,
            summary_token_count: 512,
        }];
        assert!(verify_archival_layer_vs_session(&events, &session).is_none());
    }

    #[test]
    fn rejects_invalid_archival_range() {
        let events = vec![KernelEvent::CompactionArtifactCreated {
            turn_id: "t1".into(),
            artifact_id: "art-bad".into(),
            replaced_range: MessageRange { from: 5, to: 2 },
            summary_token_count: 10,
        }];
        assert!(verify_archival_artifact_field_coherence(&events).is_some());
    }

    #[test]
    fn scratchpad_fixture_archival_timeline_aligns_with_layer() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/scratchpad_compaction.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        assert!(verify_archival_artifact_field_coherence(&events).is_none());
        let session = vec![SessionCompactionArtifactEntry {
            artifact_id: "art-golden-001".into(),
            replaced_start: 2,
            replaced_end: 9,
            messages_removed_count: 7,
            summary_token_count: 256,
        }];
        assert!(verify_archival_layer_vs_session(&events, &session).is_none());
    }

    #[test]
    fn session_mismatch_detected() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 1,
            },
            KernelEvent::CompactionArtifactCreated {
                turn_id: "t1".into(),
                artifact_id: "art-1".into(),
                replaced_range: MessageRange { from: 0, to: 3 },
                summary_token_count: 100,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 0,
            },
        ];
        let bad = SessionCompactionArtifactEntry {
            artifact_id: "art-other".into(),
            replaced_start: 0,
            replaced_end: 4,
            messages_removed_count: 4,
            summary_token_count: 100,
        };
        assert!(verify_archival_layer_vs_session(&events, &[bad]).is_some());
    }
}
