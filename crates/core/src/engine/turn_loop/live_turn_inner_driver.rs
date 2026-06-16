//! Live inner-step effect driver — `LiveTurnMachine` plans, runtime interprets.
//!
//! Phase 3b batch 5 closure: inner-step IO follows the machine effect sequence
//! (`QueryMemory` → `CallModel` → `RequestApproval?` → `ExecuteBatch` → `NotifyLsp`).

use crate::engine::kernel_event::KernelEvent;
use crate::engine::turn_loop::live_turn_inner_planner::{
    InnerStepEffectPlan, plan_v3_inner_step_baseline, plan_v3_inner_step_post_call_model,
};
use crate::engine::turn_loop::memory_plane_episodic_policy::MemoryPlaneEpisodicHints;
use crate::engine::turn_machine::{
    Effect, TurnKernelProjection, notify_lsp_effects_from_step_events,
};

/// Full baseline inner-step plan for one live iteration (through `CallModel`).
#[derive(Debug, Clone)]
pub struct InnerStepLiveEffectPlan {
    pub baseline: InnerStepEffectPlan,
}

/// Plan inner-step effects from projection (pure; no IO).
#[must_use]
pub fn plan_inner_step_live_effects(
    projection: &TurnKernelProjection,
    token_budget: u32,
    episodic_hints: Option<MemoryPlaneEpisodicHints>,
) -> InnerStepLiveEffectPlan {
    InnerStepLiveEffectPlan {
        baseline: plan_v3_inner_step_baseline(projection, token_budget, episodic_hints),
    }
}

/// Post-`CallModel` execute tail (`ExecuteBatch` per call id).
#[must_use]
pub fn inner_step_execute_batch_effects(token_budget: u32, call_ids: &[String]) -> Vec<Effect> {
    plan_v3_inner_step_post_call_model(token_budget, call_ids)
        .into_iter()
        .skip(1)
        .collect()
}

/// Post-`ExecuteBatch` LSP notify tail from step events.
#[must_use]
pub fn inner_step_notify_lsp_effects(step_events: &[KernelEvent]) -> Vec<Effect> {
    notify_lsp_effects_from_step_events(step_events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_tail_skips_leading_call_model() {
        let tail = inner_step_execute_batch_effects(8192, &["c1".into(), "c2".into()]);
        assert_eq!(tail.len(), 2);
        assert!(
            tail.iter()
                .all(|effect| matches!(effect, Effect::ExecuteBatch { .. }))
        );
    }

    #[test]
    fn live_plan_matches_baseline_planner() {
        let projection = TurnKernelProjection::default();
        let live = plan_inner_step_live_effects(&projection, 4096, None);
        assert!(matches!(
            live.baseline.call_model,
            Effect::CallModel { token_budget: 4096 }
        ));
    }
}
