//! v3 outer-loop boundary grant tracking + turn-end verify.

use tracing::warn;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_loop::continuation_boundary_policy::OuterBoundaryKind;

/// Per-turn v3 outer-boundary grant counts (reset at turn start).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct V3OuterBoundaryTurnGrants {
    pub step_limit: u32,
    pub loop_guard: u32,
    pub overflow_handoff: u32,
    pub in_turn_cycle: u32,
    pub pre_request_capacity_hold: u32,
    pub error_escalation_capacity_hold: u32,
}

impl V3OuterBoundaryTurnGrants {
    pub fn record(&mut self, kind: OuterBoundaryKind) {
        match kind {
            OuterBoundaryKind::StepLimit => self.step_limit = self.step_limit.saturating_add(1),
            OuterBoundaryKind::LoopGuard => self.loop_guard = self.loop_guard.saturating_add(1),
            OuterBoundaryKind::ContextOverflowCycleHandoff => {
                self.overflow_handoff = self.overflow_handoff.saturating_add(1);
            }
            OuterBoundaryKind::InTurnCycleAdvance => {
                self.in_turn_cycle = self.in_turn_cycle.saturating_add(1);
            }
            OuterBoundaryKind::PreRequestCapacityHold => {
                self.pre_request_capacity_hold = self.pre_request_capacity_hold.saturating_add(1);
            }
            OuterBoundaryKind::ErrorEscalationCapacityHold => {
                self.error_escalation_capacity_hold =
                    self.error_escalation_capacity_hold.saturating_add(1);
            }
        }
    }

    fn logged(&self, kind: OuterBoundaryKind) -> u32 {
        match kind {
            OuterBoundaryKind::StepLimit => self.step_limit,
            OuterBoundaryKind::LoopGuard => self.loop_guard,
            OuterBoundaryKind::ContextOverflowCycleHandoff => self.overflow_handoff,
            OuterBoundaryKind::InTurnCycleAdvance => self.in_turn_cycle,
            OuterBoundaryKind::PreRequestCapacityHold => self.pre_request_capacity_hold,
            OuterBoundaryKind::ErrorEscalationCapacityHold => self.error_escalation_capacity_hold,
        }
    }
}

fn count_event_grants(events: &[KernelEvent], kind: OuterBoundaryKind) -> u32 {
    use zagens_core::engine::kernel_event::OverflowStrategy;
    events
        .iter()
        .filter(|event| match kind {
            OuterBoundaryKind::StepLimit => {
                matches!(event, KernelEvent::StepLimitContinuation { .. })
            }
            OuterBoundaryKind::LoopGuard => {
                matches!(event, KernelEvent::LoopGuardContinuation { .. })
            }
            OuterBoundaryKind::ContextOverflowCycleHandoff => matches!(
                event,
                KernelEvent::ContextOverflowRecovered {
                    strategy: OverflowStrategy::CycleHandoff,
                    ..
                }
            ),
            OuterBoundaryKind::InTurnCycleAdvance => {
                matches!(event, KernelEvent::CycleAdvanced { .. })
            }
            OuterBoundaryKind::PreRequestCapacityHold
            | OuterBoundaryKind::ErrorEscalationCapacityHold => false,
        })
        .count() as u32
}

pub fn verify_turn_outer_boundary_grants(
    events: &[KernelEvent],
    logged: &V3OuterBoundaryTurnGrants,
) {
    let kinds = [
        OuterBoundaryKind::StepLimit,
        OuterBoundaryKind::LoopGuard,
        OuterBoundaryKind::ContextOverflowCycleHandoff,
        OuterBoundaryKind::InTurnCycleAdvance,
    ];
    let mut summaries = Vec::new();
    for kind in kinds {
        let logged_count = logged.logged(kind);
        let emitted = count_event_grants(events, kind);
        if logged_count != emitted {
            summaries.push(format!("{kind:?} logged={logged_count} events={emitted}"));
        }
    }
    if summaries.is_empty() {
        return;
    }
    warn!(
        target: "kernel_outer_boundary",
        summary = summaries.join("; "),
        "outer boundary grant diff"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::engine::kernel_event::TurnOutcome;
    use zagens_core::turn::TurnLoopMode;

    #[test]
    fn turn_grants_match_event_counts() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 20,
            },
            KernelEvent::StepLimitContinuation {
                turn_id: "t1".into(),
                step_idx: 20,
                lht_objective_injected: true,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 21,
            },
        ];
        let mut logged = V3OuterBoundaryTurnGrants::default();
        logged.record(OuterBoundaryKind::StepLimit);
        assert_eq!(
            count_event_grants(&events, OuterBoundaryKind::StepLimit),
            logged.step_limit
        );
    }
}
