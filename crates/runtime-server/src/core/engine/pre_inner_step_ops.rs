//! v3 pre-inner-step planner execution (Phase 3b batch 5b).

use zagens_core::chat::LlmClient;
use zagens_core::context_profile::auto_compaction_allowed;
use zagens_core::engine::turn_loop::live_turn_outer_planner::{
    plan_v3_pre_inner_step_baseline, pre_inner_step_baseline_effect_label,
};
use zagens_core::engine::turn_machine::Effect;

use super::compaction_ops::RunCompactionScope;
use super::effect_interpreter::EffectInterpreter;
use super::scratchpad_flow;
use super::*;
use crate::compaction::should_compact;

impl Engine {
    fn log_v3_planner_baseline_slot(turn_id: &str, step: u32, slot: usize, effect: &str) {
        tracing::info!(
            target: "kernel_v3",
            turn_id = %turn_id,
            step,
            slot,
            effect,
            "v3 planner baseline effect (EffectInterpreter)"
        );
    }

    fn should_run_in_turn_auto_compaction(&self) -> bool {
        let compaction_pins = self
            .session
            .working_set
            .pinned_message_indices(&self.session.messages, &self.session.workspace);
        let mut compaction_paths = self.session.working_set.top_paths(24);
        scratchpad_flow::extend_compaction_paths(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
            &mut compaction_paths,
        );
        self.config.compaction.enabled
            && auto_compaction_allowed(&self.session.model, &self.config.cycle)
            && should_compact(
                &self.session.messages,
                &self.config.compaction,
                Some(&self.session.workspace),
                Some(&compaction_pins),
                Some(&compaction_paths),
            )
    }

    /// Planner baseline: `RunCompaction` (slot 0) then `RunLayeredContextCheckpoint` (slot 1).
    pub(in crate::core::engine) async fn run_v3_pre_inner_step_baseline(
        &mut self,
        _client: &dyn LlmClient,
        turn_id: &str,
        step: u32,
    ) {
        let plan = plan_v3_pre_inner_step_baseline();
        let should_compact = self.should_run_in_turn_auto_compaction();

        for (slot, effect) in plan.baseline.iter().enumerate() {
            if slot == 0 && matches!(effect, Effect::RunCompaction) && !should_compact {
                continue;
            }
            if let Some(label) = pre_inner_step_baseline_effect_label(slot) {
                Self::log_v3_planner_baseline_slot(turn_id, step, slot, label);
            }
            match effect {
                Effect::RunCompaction => {
                    self.runtime_ext_mut().kernel_run_compaction_scope =
                        Some(RunCompactionScope::InTurnAuto);
                }
                Effect::RunLayeredContextCheckpoint => {}
                _ => {}
            }
            let mut interpreter = EffectInterpreter::new(self);
            let _ = interpreter.interpret(effect.clone()).await;
        }
    }

    /// Planner baseline slot 0: in-turn auto-compaction via `RunCompaction`.
    pub(in crate::core::engine) async fn run_v3_planner_auto_compaction(
        &mut self,
        _client: &dyn LlmClient,
        turn_id: &str,
        step: u32,
    ) {
        if !self.should_run_in_turn_auto_compaction() {
            return;
        }
        Self::log_v3_planner_baseline_slot(turn_id, step, 0, "RunCompaction");
        self.runtime_ext_mut().kernel_run_compaction_scope = Some(RunCompactionScope::InTurnAuto);
        let mut interpreter = EffectInterpreter::new(self);
        let _ = interpreter.interpret(Effect::RunCompaction).await;
    }

    /// Planner baseline slot 1: layered context seam via `RunLayeredContextCheckpoint`.
    pub(in crate::core::engine) async fn run_v3_planner_layered_context(
        &mut self,
        turn_id: &str,
        step: u32,
    ) {
        Self::log_v3_planner_baseline_slot(turn_id, step, 1, "RunLayeredContextCheckpoint");
        let mut interpreter = EffectInterpreter::new(self);
        let _ = interpreter
            .interpret(Effect::RunLayeredContextCheckpoint)
            .await;
    }
}
