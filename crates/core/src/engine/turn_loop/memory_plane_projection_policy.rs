//! Memory Plane layer taxonomy and replay projection (Phase 3b batch 4 / 8a).

use crate::engine::kernel_event::KernelEvent;
use crate::engine::turn_machine::TurnKernelProjection;

/// Design §3.4 layers — kernel-log rebuild substrate (no IO).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryPlaneLayer {
    /// Scratchpad, steer, cycle briefing, continuation nudges (turn-local working memory).
    Working,
    /// Topic / episodic retrieval (TopicMemory graph injection).
    Episodic,
    /// Compaction artifacts and archival summaries.
    Archival,
}

impl MemoryPlaneLayer {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Episodic => "episodic",
            Self::Archival => "archival",
        }
    }
}

/// Per-layer event counts derived from a turn log or projection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryPlaneLayerCounts {
    pub working: u32,
    pub episodic: u32,
    pub archival: u32,
}

impl MemoryPlaneLayerCounts {
    #[must_use]
    pub const fn get(self, layer: MemoryPlaneLayer) -> u32 {
        match layer {
            MemoryPlaneLayer::Working => self.working,
            MemoryPlaneLayer::Episodic => self.episodic,
            MemoryPlaneLayer::Archival => self.archival,
        }
    }

    #[must_use]
    pub const fn total(self) -> u32 {
        self.working
            .saturating_add(self.episodic)
            .saturating_add(self.archival)
    }
}

/// Count memory-plane kernel events grouped by layer.
#[must_use]
pub fn count_memory_plane_layers(events: &[KernelEvent]) -> MemoryPlaneLayerCounts {
    let mut out = MemoryPlaneLayerCounts::default();
    let mut summary_seen = false;
    for event in events {
        match event {
            KernelEvent::ScratchpadReminderInjected { .. }
            | KernelEvent::CycleBriefingInjected { .. }
            | KernelEvent::SteerInjected { .. }
            | KernelEvent::StepLimitContinuation { .. }
            | KernelEvent::LoopGuardContinuation { .. } => {
                out.working += 1;
            }
            KernelEvent::ScratchpadSummaryInjected { .. } => {
                if !summary_seen {
                    summary_seen = true;
                    out.working += 1;
                }
            }
            KernelEvent::CompactionArtifactCreated { .. } => {
                out.archival += 1;
            }
            KernelEvent::TopicMemoryInjected { .. } => {
                out.episodic += 1;
            }
            _ => {}
        }
    }
    out
}

/// Derive layer counts from [`TurnKernelProjection`] (must match log counters).
#[must_use]
pub fn memory_plane_layer_counts_from_projection(
    projection: &TurnKernelProjection,
) -> MemoryPlaneLayerCounts {
    let summary = u32::from(projection.scratchpad_summary_injected);
    MemoryPlaneLayerCounts {
        working: projection
            .scratchpad_reminder_count
            .saturating_add(summary)
            .saturating_add(projection.cycle_briefing_count)
            .saturating_add(projection.steer_injection_count)
            .saturating_add(projection.step_limit_continuations)
            .saturating_add(projection.loop_guard_continuations),
        episodic: projection.topic_memory_injection_count,
        archival: projection.compaction_artifact_count,
    }
}

/// Verify projection layer totals match the event log.
#[must_use]
pub fn verify_memory_plane_layer_coherence(events: &[KernelEvent]) -> Option<String> {
    if count_memory_plane_layers(events).total() == 0 {
        return None;
    }
    let log = count_memory_plane_layers(events);
    let projection = TurnKernelProjection::from_events(events);
    let proj = memory_plane_layer_counts_from_projection(&projection);
    let mut diffs = Vec::new();
    for layer in [
        MemoryPlaneLayer::Working,
        MemoryPlaneLayer::Episodic,
        MemoryPlaneLayer::Archival,
    ] {
        let log_n = log.get(layer);
        let proj_n = proj.get(layer);
        if log_n != proj_n {
            diffs.push(format!("{} log={log_n} proj={proj_n}", layer.as_str()));
        }
    }
    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::kernel_event::{MessageRange, TurnOutcome};
    use crate::turn::TurnLoopMode;

    #[test]
    fn scratchpad_compaction_fixture_layer_coherence_passes() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/scratchpad_compaction.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let log = count_memory_plane_layers(&events);
        assert_eq!(log.working, 3);
        assert_eq!(log.archival, 1);
        assert_eq!(log.episodic, 0);
        assert!(verify_memory_plane_layer_coherence(&events).is_none());
    }

    #[test]
    fn layer_counts_match_projection_fields() {
        let events = vec![
            KernelEvent::ScratchpadReminderInjected {
                turn_id: "t1".into(),
                step_idx: 1,
                area_path: "a.rs".into(),
            },
            KernelEvent::CompactionArtifactCreated {
                turn_id: "t1".into(),
                artifact_id: "art-1".into(),
                replaced_range: MessageRange { from: 0, to: 3 },
                summary_token_count: 100,
            },
            KernelEvent::SteerInjected {
                turn_id: "t1".into(),
                step_idx: 1,
                text: "nudge".into(),
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        let projection = TurnKernelProjection::from_events(&events);
        assert_eq!(
            memory_plane_layer_counts_from_projection(&projection),
            count_memory_plane_layers(&events)
        );
    }

    #[test]
    fn continuation_rows_count_as_working_layer() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 5,
            },
            KernelEvent::StepLimitContinuation {
                turn_id: "t1".into(),
                step_idx: 3,
                lht_objective_injected: true,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 3,
            },
        ];
        let log = count_memory_plane_layers(&events);
        assert_eq!(log.working, 1);
        assert!(verify_memory_plane_layer_coherence(&events).is_none());
    }
}
