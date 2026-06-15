//! Kernel-v2 shadow counters for corpus bake and ops monitoring (M3/M4).

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::config::{KernelMachineMode, ToolsPolicyMode, ToolsSchedulerMode};
use crate::core::engine::kernel_effect_shadow::kernel_effect_shadow_stats;
use crate::core::engine::kernel_guard_shadow::kernel_guard_shadow_stats;
use crate::core::engine::kernel_memory_shadow::kernel_memory_shadow_stats;
use crate::core::engine::kernel_message_compaction_shadow::kernel_message_compaction_shadow_stats;
use crate::core::engine::kernel_message_coverage_shadow::kernel_message_coverage_shadow_stats;
use crate::core::engine::kernel_message_memory_plane_shadow::kernel_message_memory_plane_shadow_stats;
use crate::core::engine::kernel_message_role_shadow::kernel_message_role_shadow_stats;
use crate::core::engine::kernel_message_timeline_shadow::kernel_message_timeline_shadow_stats;
use crate::core::engine::kernel_projection_shadow::kernel_projection_shadow_stats;
use crate::core::engine::kernel_replay_shadow::kernel_replay_shadow_stats;
use crate::core::engine::kernel_v3_effect_shadow::kernel_v3_effect_shadow_stats;

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
