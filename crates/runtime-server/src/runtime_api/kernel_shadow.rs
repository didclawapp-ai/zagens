//! Kernel-v2 shadow counters for corpus bake and ops monitoring (M3/M4).

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::config::{KernelMachineMode, ToolsPolicyMode, ToolsSchedulerMode};
use crate::core::engine::kernel_capacity_tail_shadow::kernel_capacity_tail_shadow_stats;
use crate::core::engine::kernel_compaction_artifact_shadow::kernel_compaction_artifact_shadow_stats;
use crate::core::engine::kernel_compaction_replay_anchor_shadow::kernel_compaction_replay_anchor_shadow_stats;
use crate::core::engine::kernel_continuation_anchor_shadow::kernel_continuation_anchor_shadow_stats;
use crate::core::engine::kernel_effect_shadow::kernel_effect_shadow_stats;
use crate::core::engine::kernel_guard_shadow::kernel_guard_shadow_stats;
use crate::core::engine::kernel_memory_plane_replay_anchor_shadow::kernel_memory_plane_replay_anchor_shadow_stats;
use crate::core::engine::kernel_memory_shadow::kernel_memory_shadow_stats;
use crate::core::engine::kernel_message_compaction_shadow::kernel_message_compaction_shadow_stats;
use crate::core::engine::kernel_message_coverage_shadow::kernel_message_coverage_shadow_stats;
use crate::core::engine::kernel_message_memory_plane_shadow::kernel_message_memory_plane_shadow_stats;
use crate::core::engine::kernel_message_role_shadow::kernel_message_role_shadow_stats;
use crate::core::engine::kernel_message_timeline_shadow::kernel_message_timeline_shadow_stats;
use crate::core::engine::kernel_notify_lsp_anchor_shadow::kernel_notify_lsp_anchor_shadow_stats;
use crate::core::engine::kernel_outer_boundary_shadow::kernel_outer_boundary_shadow_stats;
use crate::core::engine::kernel_pre_inner_step_baseline_shadow::kernel_pre_inner_step_baseline_shadow_stats;
use crate::core::engine::kernel_projection_shadow::kernel_projection_shadow_stats;
use crate::core::engine::kernel_replay_shadow::kernel_replay_shadow_stats;
use crate::core::engine::kernel_request_approval_anchor_shadow::kernel_request_approval_anchor_shadow_stats;
use crate::core::engine::kernel_resume_replay_anchor_shadow::{
    kernel_resume_replay_anchor_alignment_stats, kernel_resume_replay_anchor_shadow_stats,
};
use crate::core::engine::kernel_v3_effect_shadow::kernel_v3_effect_shadow_stats;
use crate::core::engine::kernel_v3_replay_counts::v3_last_replay_effect_counts;

use super::{ApiError, RuntimeApiState};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ShadowCounterBlock {
    mode: String,
    comparisons: u64,
    diffs: u64,
    diff_rate_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReplayShadowBlock {
    mode: String,
    comparisons: u64,
    diffs: u64,
    persist_diffs: u64,
    diff_rate_pct: f64,
    persist_diff_rate_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct V3ReplayEffectCountsBlock {
    mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_turn_id: Option<String>,
    call_model: u32,
    execute_batch: u32,
    request_approval: u32,
    inject_steer: u32,
    run_compaction: u32,
    notify_lsp: u32,
    sleep: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ResumeReplayAnchorShadowBlock {
    mode: String,
    resume_runs: u64,
    turns_interpreted: u64,
    anchors_interpreted: u64,
    turns_skipped: u64,
    anchor_alignment_checks: u64,
    anchor_alignment_diffs: u64,
    anchor_alignment_diff_rate_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PreInnerStepBaselineShadowBlock {
    mode: String,
    baseline_steps: u64,
    slot0_interpreter: u64,
    slot1_interpreter: u64,
    slot0_skipped_pre_interpreter: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KernelShadowResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    policy_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scheduler_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    projection_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effect_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    guard_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    replay_shadow: Option<ReplayShadowBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    v3_effect_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_coverage_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_timeline_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_role_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_memory_plane_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_compaction_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compaction_artifact_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    continuation_anchor_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notify_lsp_anchor_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_approval_anchor_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_plane_replay_anchor_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compaction_replay_anchor_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    v3_replay_effect_counts: Option<V3ReplayEffectCountsBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resume_replay_anchor_shadow: Option<ResumeReplayAnchorShadowBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capacity_tail_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pre_inner_step_baseline_shadow: Option<PreInnerStepBaselineShadowBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outer_boundary_shadow: Option<ShadowCounterBlock>,
}

pub(crate) async fn kernel_shadow_stats(
    State(state): State<RuntimeApiState>,
) -> Result<Json<KernelShadowResponse>, ApiError> {
    Ok(Json(collect_kernel_shadow_stats(&state.config)))
}

pub(crate) fn collect_kernel_shadow_stats(config: &crate::config::Config) -> KernelShadowResponse {
    let policy_shadow = probe_policy_shadow(config);
    let scheduler_shadow = probe_scheduler_shadow(config);
    let projection_shadow = probe_projection_shadow();
    let effect_shadow = probe_effect_shadow(config);
    let guard_shadow = probe_guard_shadow(config);
    let memory_shadow = probe_memory_shadow(config);
    let replay_shadow = probe_replay_shadow(config);
    let v3_effect_shadow = probe_v3_effect_shadow(config);
    let message_coverage_shadow = probe_message_coverage_shadow(config);
    let message_timeline_shadow = probe_message_timeline_shadow(config);
    let message_role_shadow = probe_message_role_shadow(config);
    let message_memory_plane_shadow = probe_message_memory_plane_shadow(config);
    let message_compaction_shadow = probe_message_compaction_shadow(config);
    let compaction_artifact_shadow = probe_compaction_artifact_shadow(config);
    let continuation_anchor_shadow = probe_continuation_anchor_shadow(config);
    let notify_lsp_anchor_shadow = probe_notify_lsp_anchor_shadow(config);
    let request_approval_anchor_shadow = probe_request_approval_anchor_shadow(config);
    let memory_plane_replay_anchor_shadow = probe_memory_plane_replay_anchor_shadow(config);
    let compaction_replay_anchor_shadow = probe_compaction_replay_anchor_shadow(config);
    let v3_replay_effect_counts = probe_v3_replay_effect_counts(config);
    let resume_replay_anchor_shadow = probe_resume_replay_anchor_shadow(config);
    let capacity_tail_shadow = probe_capacity_tail_shadow(config);
    let pre_inner_step_baseline_shadow = probe_pre_inner_step_baseline_shadow(config);
    let outer_boundary_shadow = probe_outer_boundary_shadow(config);
    KernelShadowResponse {
        policy_shadow,
        scheduler_shadow,
        projection_shadow,
        effect_shadow,
        guard_shadow,
        memory_shadow,
        replay_shadow,
        v3_effect_shadow,
        message_coverage_shadow,
        message_timeline_shadow,
        message_role_shadow,
        message_memory_plane_shadow,
        message_compaction_shadow,
        compaction_artifact_shadow,
        continuation_anchor_shadow,
        notify_lsp_anchor_shadow,
        request_approval_anchor_shadow,
        memory_plane_replay_anchor_shadow,
        compaction_replay_anchor_shadow,
        v3_replay_effect_counts,
        resume_replay_anchor_shadow,
        capacity_tail_shadow,
        pre_inner_step_baseline_shadow,
        outer_boundary_shadow,
    }
}

fn probe_policy_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.tools_policy_mode();
    if mode != ToolsPolicyMode::Shadow {
        return None;
    }
    let stats = zagens_tools::policy_shadow_stats();
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons: stats.comparisons,
        diffs: stats.diffs,
        diff_rate_pct: shadow_diff_rate_pct(stats.comparisons, stats.diffs),
    })
}

fn probe_scheduler_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.tools_scheduler_mode();
    if mode != ToolsSchedulerMode::Shadow {
        return None;
    }
    let stats = zagens_tools::scheduler_shadow_stats();
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons: stats.comparisons,
        diffs: stats.diffs,
        diff_rate_pct: shadow_diff_rate_pct(stats.comparisons, stats.diffs),
    })
}

fn shadow_diff_rate_pct(comparisons: u64, diffs: u64) -> f64 {
    if comparisons == 0 {
        0.0
    } else {
        (diffs as f64 / comparisons as f64) * 100.0
    }
}

fn probe_projection_shadow() -> Option<ShadowCounterBlock> {
    let (comparisons, diffs) = kernel_projection_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: "shadow".to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_effect_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if mode != KernelMachineMode::Shadow {
        return None;
    }
    let (comparisons, diffs) = kernel_effect_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_guard_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if mode != KernelMachineMode::Shadow {
        return None;
    }
    let (comparisons, diffs) = kernel_guard_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_memory_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if mode != KernelMachineMode::Shadow {
        return None;
    }
    let (comparisons, diffs) = kernel_memory_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_replay_shadow(config: &crate::config::Config) -> Option<ReplayShadowBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_replay_verification() {
        return None;
    }
    let (comparisons, diffs, persist_diffs) = kernel_replay_shadow_stats();
    if comparisons == 0 && diffs == 0 && persist_diffs == 0 {
        return None;
    }
    Some(ReplayShadowBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        persist_diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
        persist_diff_rate_pct: shadow_diff_rate_pct(comparisons, persist_diffs),
    })
}

fn probe_v3_effect_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_v3_turn_loop() {
        return None;
    }
    let (comparisons, diffs) = kernel_v3_effect_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_capacity_tail_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_v3_turn_loop() {
        return None;
    }
    let (comparisons, diffs) = kernel_capacity_tail_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_pre_inner_step_baseline_shadow(
    config: &crate::config::Config,
) -> Option<PreInnerStepBaselineShadowBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_v3_turn_loop() {
        return None;
    }
    let (baseline_steps, slot0_interpreter, slot1_interpreter, slot0_skipped_pre_interpreter) =
        kernel_pre_inner_step_baseline_shadow_stats();
    if baseline_steps == 0 {
        return None;
    }
    Some(PreInnerStepBaselineShadowBlock {
        mode: mode.as_str().to_string(),
        baseline_steps,
        slot0_interpreter,
        slot1_interpreter,
        slot0_skipped_pre_interpreter,
    })
}

fn probe_outer_boundary_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_v3_turn_loop() {
        return None;
    }
    let (comparisons, diffs) = kernel_outer_boundary_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_v3_replay_effect_counts(
    config: &crate::config::Config,
) -> Option<V3ReplayEffectCountsBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_v3_turn_loop() {
        return None;
    }
    let last = v3_last_replay_effect_counts();
    if last.turn_id.is_none() {
        return None;
    }
    Some(V3ReplayEffectCountsBlock {
        mode: mode.as_str().to_string(),
        last_turn_id: last.turn_id,
        call_model: last.counts.call_model,
        execute_batch: last.counts.execute_batch,
        request_approval: last.counts.request_approval,
        inject_steer: last.counts.inject_steer,
        run_compaction: last.counts.run_compaction,
        notify_lsp: last.counts.notify_lsp,
        sleep: last.counts.sleep,
    })
}

fn probe_resume_replay_anchor_shadow(
    config: &crate::config::Config,
) -> Option<ResumeReplayAnchorShadowBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_replay_verification() && !mode.uses_v3_turn_loop() {
        return None;
    }
    let (resume_runs, turns_interpreted, anchors_interpreted, turns_skipped) =
        kernel_resume_replay_anchor_shadow_stats();
    let (anchor_alignment_checks, anchor_alignment_diffs) =
        kernel_resume_replay_anchor_alignment_stats();
    if resume_runs == 0 {
        return None;
    }
    Some(ResumeReplayAnchorShadowBlock {
        mode: mode.as_str().to_string(),
        resume_runs,
        turns_interpreted,
        anchors_interpreted,
        turns_skipped,
        anchor_alignment_checks,
        anchor_alignment_diffs,
        anchor_alignment_diff_rate_pct: shadow_diff_rate_pct(
            anchor_alignment_checks,
            anchor_alignment_diffs,
        ),
    })
}

fn probe_message_coverage_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_replay_verification() {
        return None;
    }
    let (comparisons, diffs) = kernel_message_coverage_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_message_timeline_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_replay_verification() {
        return None;
    }
    let (comparisons, diffs) = kernel_message_timeline_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_message_role_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_replay_verification() {
        return None;
    }
    let (comparisons, diffs) = kernel_message_role_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_message_memory_plane_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_replay_verification() {
        return None;
    }
    let (comparisons, diffs) = kernel_message_memory_plane_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_message_compaction_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_replay_verification() {
        return None;
    }
    let (comparisons, diffs) = kernel_message_compaction_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_compaction_artifact_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_replay_verification() {
        return None;
    }
    let (comparisons, diffs) = kernel_compaction_artifact_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_continuation_anchor_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_replay_verification() {
        return None;
    }
    let (comparisons, diffs) = kernel_continuation_anchor_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_notify_lsp_anchor_shadow(config: &crate::config::Config) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_replay_verification() {
        return None;
    }
    let (comparisons, diffs) = kernel_notify_lsp_anchor_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_request_approval_anchor_shadow(
    config: &crate::config::Config,
) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_replay_verification() {
        return None;
    }
    let (comparisons, diffs) = kernel_request_approval_anchor_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_memory_plane_replay_anchor_shadow(
    config: &crate::config::Config,
) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_replay_verification() {
        return None;
    }
    let (comparisons, diffs) = kernel_memory_plane_replay_anchor_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

fn probe_compaction_replay_anchor_shadow(
    config: &crate::config::Config,
) -> Option<ShadowCounterBlock> {
    let mode = config.kernel_machine_mode();
    if !mode.uses_replay_verification() {
        return None;
    }
    let (comparisons, diffs) = kernel_compaction_replay_anchor_shadow_stats();
    if comparisons == 0 && diffs == 0 {
        return None;
    }
    Some(ShadowCounterBlock {
        mode: mode.as_str().to_string(),
        comparisons,
        diffs,
        diff_rate_pct: shadow_diff_rate_pct(comparisons, diffs),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn kernel_shadow_omits_policy_shadow_when_engine_is_default() {
        // Default config: policy=Engine, scheduler=Shadow.
        // - policy_shadow: None (Engine is default, not Shadow)
        // - scheduler_shadow: Some (Shadow IS the scheduler default — M4 bake active)
        let config = Config::default();
        let resp = collect_kernel_shadow_stats(&config);
        assert!(
            resp.policy_shadow.is_none(),
            "policy shadow absent (engine is default)"
        );
        assert!(
            resp.scheduler_shadow.is_some(),
            "scheduler shadow present (shadow is the M4 bake default)"
        );
    }
}
