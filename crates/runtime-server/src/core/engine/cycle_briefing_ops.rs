//! v3 cycle briefing — routes cycle advance through [`Effect::InjectSteer`] replay anchor.

use crate::agent_surface::AppMode;

use super::*;
use zagens_core::engine::turn_loop::continuation_boundary_policy::OuterBoundaryKind;

/// Pending live IO behind an empty-text v3 `InjectSteer` interpret call.
#[derive(Debug, Clone)]
pub(in crate::core::engine) enum InjectSteerEffectKind {
    CycleAdvance { mode: AppMode, reason: String },
    StepLimitContinuation { text: String, open_items: u32 },
    LoopGuardContinuation { text: String, open_items: u32 },
}

impl Engine {
    /// Route cycle advance through v3 planner `InjectSteer` or legacy direct IO.
    pub(in crate::core::engine) async fn route_cycle_advance(
        &mut self,
        mode: AppMode,
        reason: &str,
        boundary: Option<OuterBoundaryKind>,
    ) -> bool {
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            return self.perform_cycle_advance(mode, reason).await;
        }
        if let Some(boundary) = boundary {
            return self
                .run_v3_planner_cycle_advance(boundary, mode, reason)
                .await;
        }
        self.runtime_ext_mut().kernel_active_cycle_boundary = None;
        self.runtime_ext_mut().kernel_cycle_advance_ok = None;
        self.runtime_ext_mut().kernel_pending_inject_steer_kind =
            Some(InjectSteerEffectKind::CycleAdvance {
                mode,
                reason: reason.to_string(),
            });
        let mut interpreter = super::effect_interpreter::EffectInterpreter::new(self);
        let _ = interpreter
            .interpret(zagens_core::engine::turn_machine::Effect::InjectSteer {
                text: String::new(),
            })
            .await;
        self.runtime_ext_mut()
            .kernel_cycle_advance_ok
            .take()
            .unwrap_or(false)
    }

    /// Consume a pending inject-steer kind before normal steer text handling.
    pub(in crate::core::engine) async fn try_run_pending_inject_steer_kind(&mut self) -> bool {
        let Some(kind) = self
            .runtime_ext_mut()
            .kernel_pending_inject_steer_kind
            .take()
        else {
            return false;
        };
        if self.effect_replay_anchor_only() {
            tracing::info!(
                target: "kernel_v3",
                "replay anchor-only: skipping InjectSteer IO"
            );
            if matches!(kind, InjectSteerEffectKind::CycleAdvance { .. }) {
                self.runtime_ext_mut().kernel_cycle_advance_ok = Some(true);
            }
            return true;
        }
        match kind {
            InjectSteerEffectKind::CycleAdvance { mode, reason } => {
                let ok = self.perform_cycle_advance(mode, &reason).await;
                self.runtime_ext_mut().kernel_cycle_advance_ok = Some(ok);
                true
            }
            InjectSteerEffectKind::StepLimitContinuation { text, open_items } => {
                let turn_id = self
                    .runtime_ext()
                    .kernel_active_turn_id
                    .clone()
                    .unwrap_or_else(|| self.session.id.clone());
                let step_idx = self.runtime_ext().kernel_active_step;
                self.apply_inject_steer_text(&turn_id, step_idx, text).await;
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.step_limit_continue: {{\"open_items\":{open_items}}}"
                    )))
                    .await;
                true
            }
            InjectSteerEffectKind::LoopGuardContinuation { text, open_items } => {
                let turn_id = self
                    .runtime_ext()
                    .kernel_active_turn_id
                    .clone()
                    .unwrap_or_else(|| self.session.id.clone());
                let step_idx = self.runtime_ext().kernel_active_step;
                self.apply_inject_steer_text(&turn_id, step_idx, text).await;
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.loop_guard_continue: {{\"open_items\":{open_items}}}"
                    )))
                    .await;
                true
            }
        }
    }
}
