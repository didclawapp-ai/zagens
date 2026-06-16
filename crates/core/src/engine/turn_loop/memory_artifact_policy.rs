//! Memory-plane artifact emission (Phase 3b batch 4 / Phase D — `Effect::EmitArtifact`).

use crate::engine::kernel_event::KernelEvent;
use crate::engine::turn_machine::Effect;

/// Artifact kinds routed through [`Effect::EmitArtifact`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryArtifactKind {
    /// Turn-end scratchpad layered summary (`ScratchpadSummaryInjected`).
    ScratchpadSnapshot,
    /// Read-only-tool threshold reminder (`ScratchpadReminderInjected`).
    ScratchpadReminder,
}

impl MemoryArtifactKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ScratchpadSnapshot => "scratchpad_snapshot",
            Self::ScratchpadReminder => "scratchpad_reminder",
        }
    }
}

/// Build the v3 effect for a memory-plane artifact emission.
#[must_use]
pub fn emit_artifact_effect(kind: MemoryArtifactKind, area_hint: Option<String>) -> Effect {
    Effect::EmitArtifact { kind, area_hint }
}

/// Whether a kernel event replays through [`Effect::EmitArtifact`].
#[must_use]
pub fn is_emit_artifact_kernel_event(event: &KernelEvent) -> bool {
    matches!(
        event,
        KernelEvent::ScratchpadSummaryInjected { .. }
            | KernelEvent::ScratchpadReminderInjected { .. }
    )
}

/// Derive `EmitArtifact` replay effects from scratchpad kernel events.
#[must_use]
pub fn memory_plane_emit_artifact_effects_from_events(events: &[KernelEvent]) -> Vec<Effect> {
    events
        .iter()
        .filter_map(|event| match event {
            KernelEvent::ScratchpadSummaryInjected { .. } => Some(emit_artifact_effect(
                MemoryArtifactKind::ScratchpadSnapshot,
                None,
            )),
            KernelEvent::ScratchpadReminderInjected { area_path, .. } => {
                Some(emit_artifact_effect(
                    MemoryArtifactKind::ScratchpadReminder,
                    Some(area_path.clone()),
                ))
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratchpad_fixture_derives_emit_artifact_effects() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/scratchpad_compaction.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let effects = memory_plane_emit_artifact_effects_from_events(&events);
        assert_eq!(effects.len(), 2);
        assert!(matches!(
            &effects[0],
            Effect::EmitArtifact {
                kind: MemoryArtifactKind::ScratchpadReminder,
                ..
            }
        ));
        assert!(matches!(
            &effects[1],
            Effect::EmitArtifact {
                kind: MemoryArtifactKind::ScratchpadSnapshot,
                ..
            }
        ));
    }
}
