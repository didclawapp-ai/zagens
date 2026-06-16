//! v3 pre-inner-step planner baseline execution shadow (Phase 3b batch 5b).

use std::sync::atomic::{AtomicU64, Ordering};

static BASELINE_STEPS: AtomicU64 = AtomicU64::new(0);
static SLOT0_INTERPRETER: AtomicU64 = AtomicU64::new(0);
static SLOT1_INTERPRETER: AtomicU64 = AtomicU64::new(0);
static SLOT0_SKIPPED_PRE_INTERPRETER: AtomicU64 = AtomicU64::new(0);

/// One v3 outer-loop iteration reached the pre-inner-step baseline (both slots planned).
pub fn record_pre_inner_step_baseline_step() {
    BASELINE_STEPS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_pre_inner_step_slot0_interpreter() {
    SLOT0_INTERPRETER.fetch_add(1, Ordering::Relaxed);
}

pub fn record_pre_inner_step_slot1_interpreter() {
    SLOT1_INTERPRETER.fetch_add(1, Ordering::Relaxed);
}

/// Compaction baseline slot skipped before `EffectInterpreter` (e.g. `should_compact` false).
pub fn record_pre_inner_step_slot0_skipped_pre_interpreter() {
    SLOT0_SKIPPED_PRE_INTERPRETER.fetch_add(1, Ordering::Relaxed);
}

#[must_use]
pub fn kernel_pre_inner_step_baseline_shadow_stats() -> (u64, u64, u64, u64) {
    (
        BASELINE_STEPS.load(Ordering::Relaxed),
        SLOT0_INTERPRETER.load(Ordering::Relaxed),
        SLOT1_INTERPRETER.load(Ordering::Relaxed),
        SLOT0_SKIPPED_PRE_INTERPRETER.load(Ordering::Relaxed),
    )
}
