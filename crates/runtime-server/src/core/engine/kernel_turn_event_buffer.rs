//! Per-turn kernel event accumulator + live vs projection compare at turn end.

use tracing::warn;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_machine::{
    LiveTurnSnapshot, TurnKernelProjection, compare_projection_to_live,
};

/// Per-engine turn event accumulator (enabled when kernel event writer is present).
pub struct KernelTurnEventBuffer {
    turn_events: Vec<KernelEvent>,
    enabled: bool,
}

impl KernelTurnEventBuffer {
    pub fn new(enabled: bool) -> Self {
        Self {
            turn_events: Vec::new(),
            enabled,
        }
    }

    pub fn reset_turn(&mut self) {
        self.turn_events.clear();
    }

    pub fn record(&mut self, event: KernelEvent) {
        if self.enabled {
            self.turn_events.push(event);
        }
    }

    pub fn turn_events(&self) -> &[KernelEvent] {
        &self.turn_events
    }

    pub fn finish_turn(&mut self, live: &LiveTurnSnapshot) {
        if !self.enabled {
            return;
        }
        let projection = TurnKernelProjection::from_events(&self.turn_events);
        if let Some(summary) = compare_projection_to_live(live, &projection) {
            warn!(
                target: "kernel_turn_events",
                turn_id = %live.turn_id,
                summary,
                "projection vs live diff at turn end"
            );
        }
        self.turn_events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::turn::TurnLoopMode;

    #[test]
    fn finish_turn_warns_when_mismatch() {
        let mut buffer = KernelTurnEventBuffer::new(true);
        buffer.record(KernelEvent::TurnStarted {
            turn_id: "t1".into(),
            mode: TurnLoopMode::Agent,
            input_text: "hi".into(),
            max_steps: 10,
        });
        let live = LiveTurnSnapshot {
            turn_id: "t1".into(),
            step_idx: 99,
            max_steps: 10,
            ..Default::default()
        };
        buffer.finish_turn(&live);
        assert!(buffer.turn_events().is_empty());
    }
}
