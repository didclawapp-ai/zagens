//! v3 capacity checkpoint effect-tail shadow (Phase 3b batch 5b).

use std::sync::atomic::{AtomicU64, Ordering};

use tracing::warn;
use zagens_core::capacity::GuardrailAction;
use zagens_core::engine::turn_loop::live_turn_outer_planner::{
    CapacityCheckpointEffectTail, plan_capacity_checkpoint_effect_tail,
    verify_capacity_tail_alignment,
};

static COMPARISONS: AtomicU64 = AtomicU64::new(0);
static DIFFS: AtomicU64 = AtomicU64::new(0);

pub fn record_capacity_tail_shadow(
    action: GuardrailAction,
    cooldown_blocked: bool,
    interpreted: CapacityCheckpointEffectTail,
) {
    let planned = plan_capacity_checkpoint_effect_tail(action, cooldown_blocked);
    COMPARISONS.fetch_add(1, Ordering::Relaxed);
    if let Some(summary) = verify_capacity_tail_alignment(planned, interpreted) {
        DIFFS.fetch_add(1, Ordering::Relaxed);
        warn!(
            target: "kernel_capacity_tail_shadow",
            %summary,
            ?action,
            cooldown_blocked,
            "capacity tail shadow diff"
        );
    }
}

#[must_use]
pub fn kernel_capacity_tail_shadow_stats() -> (u64, u64) {
    (
        COMPARISONS.load(Ordering::Relaxed),
        DIFFS.load(Ordering::Relaxed),
    )
}
