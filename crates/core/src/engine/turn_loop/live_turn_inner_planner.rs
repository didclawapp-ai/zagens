//! Phase 3b batch 5b cont. — live inner-step effect planner.
//!
//! Documents the v3 inner step chain (`QueryMemory` → `CallModel` → `ExecuteBatch` → `NotifyLsp`).
//! Live IO runs through [`EffectInterpreter`] + [`LiveTurnMachine::inner_step_live_plan`](super::live_turn_machine::LiveTurnMachine::inner_step_live_plan).

use crate::engine::turn_loop::memory_plane_episodic_policy::MemoryPlaneEpisodicHints;
use crate::engine::turn_machine::{
    Effect, TurnKernelProjection, plan_v3_pre_call_model_effects, plan_v3_step_effects,
};

/// Baseline v3 inner-step effect plan before tool call ids are known.
#[derive(Debug, Clone)]
pub struct InnerStepEffectPlan {
    pub pre_call_model: Vec<Effect>,
    pub call_model: Effect,
    /// Post-`CallModel`: one `ExecuteBatch` per planned tool call id.
    pub execute_batch_per_call: bool,
    /// Post-`ExecuteBatch`: `NotifyLsp` tail from step events when edit tools ran.
    pub notify_lsp_tail: bool,
}

/// Canonical v3 inner-step baseline (pre-call queries + `CallModel`; dynamic tail documented).
#[must_use]
pub fn plan_v3_inner_step_baseline(
    projection: &TurnKernelProjection,
    token_budget: u32,
    episodic_hints: Option<MemoryPlaneEpisodicHints>,
) -> InnerStepEffectPlan {
    InnerStepEffectPlan {
        pre_call_model: plan_v3_pre_call_model_effects(projection, episodic_hints),
        call_model: Effect::CallModel { token_budget },
        execute_batch_per_call: true,
        notify_lsp_tail: true,
    }
}

/// Full post-`CallModel` plan once tool call ids are known (includes leading `CallModel`).
#[must_use]
pub fn plan_v3_inner_step_post_call_model(token_budget: u32, call_ids: &[String]) -> Vec<Effect> {
    plan_v3_step_effects(token_budget, call_ids)
}

/// Baseline effects through `CallModel` (excludes dynamic `ExecuteBatch` / `NotifyLsp`).
#[must_use]
pub fn inner_step_baseline_effects(plan: &InnerStepEffectPlan) -> Vec<Effect> {
    let mut effects = plan.pre_call_model.clone();
    effects.push(plan.call_model.clone());
    effects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_step_baseline_includes_call_model() {
        let projection = TurnKernelProjection::default();
        let plan = plan_v3_inner_step_baseline(&projection, 8192, None);
        assert!(plan.execute_batch_per_call);
        assert!(plan.notify_lsp_tail);
        let baseline = inner_step_baseline_effects(&plan);
        assert!(matches!(
            baseline.last(),
            Some(Effect::CallModel { token_budget: 8192 })
        ));
    }

    #[test]
    fn post_call_model_plan_matches_step_effects_helper() {
        let ids = vec!["call-a".into(), "call-b".into()];
        let plan = plan_v3_inner_step_post_call_model(4096, &ids);
        assert_eq!(plan.len(), 3);
        assert!(matches!(plan[0], Effect::CallModel { token_budget: 4096 }));
        assert!(matches!(plan[1], Effect::ExecuteBatch { .. }));
        assert!(matches!(plan[2], Effect::ExecuteBatch { .. }));
    }
}
