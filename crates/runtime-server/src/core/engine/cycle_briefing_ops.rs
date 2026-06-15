//! v3 cycle briefing — routes cycle advance through [`Effect::InjectSteer`] replay anchor.

use crate::agent_surface::AppMode;

use super::effect_interpreter::EffectInterpreter;
use super::*;

use zagens_core::engine::turn_machine::Effect;

/// Pending live IO behind an empty-text v3 `InjectSteer` interpret call.
#[derive(Debug, Clone)]
pub(in crate::core::engine) enum InjectSteerEffectKind {
    CycleAdvance { mode: AppMode, reason: String },
}

impl Engine {
    /// Route cycle advance through v3 empty `InjectSteer` anchor or legacy direct IO.
    pub(in crate::core::engine) async fn route_cycle_advance(
        &mut self,
        mode: AppMode,
        reason: &str,
    ) -> bool {
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            return self.perform_cycle_advance(mode, reason).await;
        }
        tracing::info!(
            target: "kernel_v3",
            reason,
            from = self.session.cycle_count,
            to = self.session.cycle_count.saturating_add(1),
            "v3 cycle: InjectSteer briefing anchor (effect plan)"
        );
        let ext = self.runtime_ext_mut();
        ext.kernel_pending_inject_steer_kind = Some(InjectSteerEffectKind::CycleAdvance {
            mode,
            reason: reason.to_string(),
        });
        ext.kernel_cycle_advance_ok = None;
        let mut interpreter = EffectInterpreter::new(self);
        let _ = interpreter
            .interpret(Effect::InjectSteer {
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
                "replay anchor-only: skipping cycle advance IO"
            );
            self.runtime_ext_mut().kernel_cycle_advance_ok = Some(true);
            return true;
        }
        match kind {
            InjectSteerEffectKind::CycleAdvance { mode, reason } => {
                let ok = self.perform_cycle_advance(mode, &reason).await;
                self.runtime_ext_mut().kernel_cycle_advance_ok = Some(ok);
                true
            }
        }
    }
}
