//! Turn-end replay coherence + optional SQLite persist verify.

use std::time::Duration;

use tracing::warn;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_machine::{LiveTurnSnapshot, verify_turn_replay_coherence};
use zagens_runtime_adapters::persist::KernelEventWriter;

pub struct KernelTurnReplayVerify {
    enabled: bool,
}

impl KernelTurnReplayVerify {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// In-memory replay gate (sync — called at turn end from `run.rs`).
    pub fn verify_turn_in_memory(&self, events: &[KernelEvent], live: &LiveTurnSnapshot) {
        if !self.enabled {
            return;
        }
        if let Some(summary) = verify_turn_replay_coherence(events, Some(live)) {
            warn!(
                target: "kernel_turn_replay",
                turn_id = %live.turn_id,
                summary,
                "turn replay coherence diff"
            );
        }
    }

    /// SQLite round-trip check (async — drain task may lag slightly behind turn end).
    pub async fn verify_turn_persisted(
        &self,
        writer: &KernelEventWriter,
        turn_id: &str,
        in_memory: &[KernelEvent],
    ) {
        if !self.enabled {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Some(summary) = writer.verify_persisted_turn_matches(turn_id, in_memory) {
            warn!(
                target: "kernel_turn_replay",
                turn_id,
                summary,
                "persisted replay diff"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::engine::kernel_event::TurnOutcome;
    use zagens_core::turn::TurnLoopMode;

    #[test]
    fn verify_turn_in_memory_passes_minimal_turn() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "hi".into(),
                max_steps: 5,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        let live = LiveTurnSnapshot {
            turn_id: "t1".into(),
            max_steps: 5,
            ..Default::default()
        };
        let verify = KernelTurnReplayVerify::new(true);
        verify.verify_turn_in_memory(&events, &live);
    }
}
