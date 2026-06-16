//! v3 pre-inner-step planner execution (Phase 3b batch 5b).

use zagens_core::engine::turn_machine::Effect;

use super::compaction_ops::RunCompactionScope;
use super::effect_interpreter::EffectInterpreter;
use super::kernel_pre_inner_step_baseline_shadow::{
    record_pre_inner_step_baseline_step, record_pre_inner_step_slot0_interpreter,
    record_pre_inner_step_slot1_interpreter,
};
use super::*;

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

    /// Planner baseline slot 0: in-turn auto-compaction via `RunCompaction`.
    pub(in crate::core::engine) async fn run_v3_planner_auto_compaction(
        &mut self,
        _client: &dyn LlmClient,
        turn_id: &str,
        step: u32,
    ) {
        Self::log_v3_planner_baseline_slot(turn_id, step, 0, "RunCompaction");
        record_pre_inner_step_slot0_interpreter();
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
        record_pre_inner_step_slot1_interpreter();
        let mut interpreter = EffectInterpreter::new(self);
        let _ = interpreter
            .interpret(Effect::RunLayeredContextCheckpoint)
            .await;
    }
}
