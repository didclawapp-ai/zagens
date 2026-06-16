//! v3 outer-loop boundary planner execution (Phase 3b batch 5b cont.).

use zagens_core::engine::turn_loop::continuation_boundary_policy::OuterBoundaryKind;
use zagens_core::engine::turn_loop::live_turn_outer_planner::plan_outer_boundary_replay_effect;
use zagens_core::engine::turn_machine::Effect;

use super::cycle_briefing_ops::InjectSteerEffectKind;
use super::effect_interpreter::EffectInterpreter;
use super::*;

impl Engine {
    fn log_v3_outer_boundary_planner_effect(kind: OuterBoundaryKind, turn_id: &str, step: u32) {
        let effect = plan_outer_boundary_replay_effect(kind)
            .map(|planned| format!("{planned:?}"))
            .unwrap_or_else(|| "none".to_string());
        tracing::info!(
            target: "kernel_v3",
            turn_id = %turn_id,
            step,
            boundary = ?kind,
            effect,
            "v3 outer boundary effect (EffectInterpreter)"
        );
    }

    /// Run replay-aligned `InjectSteer` for a continuation boundary grant.
    pub(in crate::core::engine) async fn run_v3_planner_outer_boundary_steer(
        &mut self,
        kind: OuterBoundaryKind,
        turn_id: &str,
        step: u32,
        pending: InjectSteerEffectKind,
    ) {
        Self::log_v3_outer_boundary_planner_effect(kind, turn_id, step);
        self.runtime_ext_mut().kernel_pending_inject_steer_kind = Some(pending);
        let effect = plan_outer_boundary_replay_effect(kind).unwrap_or(Effect::InjectSteer {
            text: String::new(),
        });
        let mut interpreter = EffectInterpreter::new(self);
        let _ = interpreter.interpret(effect).await;
    }

    /// Cycle handoff / in-turn advance via planner baseline `InjectSteer`.
    pub(in crate::core::engine) async fn run_v3_planner_cycle_advance(
        &mut self,
        boundary: OuterBoundaryKind,
        mode: crate::agent_surface::AppMode,
        reason: &str,
    ) -> bool {
        let turn_id = self
            .runtime_ext()
            .kernel_active_turn_id
            .clone()
            .unwrap_or_else(|| self.session.id.clone());
        let step = self.runtime_ext().kernel_active_step;
        self.runtime_ext_mut().kernel_active_cycle_boundary = Some(boundary);
        self.runtime_ext_mut().kernel_cycle_advance_ok = None;
        self.run_v3_planner_outer_boundary_steer(
            boundary,
            &turn_id,
            step,
            InjectSteerEffectKind::CycleAdvance {
                mode,
                reason: reason.to_string(),
            },
        )
        .await;
        self.runtime_ext_mut()
            .kernel_cycle_advance_ok
            .take()
            .unwrap_or(false)
    }
}
