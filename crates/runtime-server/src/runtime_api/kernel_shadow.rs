//! Kernel-v2 shadow counters for corpus bake and ops monitoring (M3/M4).

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::config::{ToolsPolicyMode, ToolsSchedulerMode};

use super::{ApiError, RuntimeApiState};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ShadowCounterBlock {
    mode: String,
    comparisons: u64,
    diffs: u64,
    diff_rate_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KernelShadowResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    policy_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scheduler_shadow: Option<ShadowCounterBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context_compiler_shadow: Option<ContextCompilerShadowBlock>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ContextCompilerShadowBlock {
    mode: String,
    comparisons: u64,
    static_diffs: u64,
    full_diffs: u64,
    static_diff_rate_pct: f64,
}

pub(crate) async fn kernel_shadow_stats(
    State(state): State<RuntimeApiState>,
) -> Result<Json<KernelShadowResponse>, ApiError> {
    Ok(Json(collect_kernel_shadow_stats(&state.config)))
}

pub(crate) fn collect_kernel_shadow_stats(config: &crate::config::Config) -> KernelShadowResponse {
    let policy_shadow = probe_policy_shadow(config);
    let scheduler_shadow = probe_scheduler_shadow(config);
    let context_compiler_shadow = probe_context_compiler_shadow(config);
    KernelShadowResponse {
        policy_shadow,
        scheduler_shadow,
        context_compiler_shadow,
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

fn probe_context_compiler_shadow(
    config: &crate::config::Config,
) -> Option<ContextCompilerShadowBlock> {
    let mode = config.context_compiler_mode();
    if mode != zagens_core::engine::ContextCompilerMode::Shadow {
        return None;
    }
    let stats = crate::context_compiler_shadow::context_compiler_shadow_stats();
    let static_diff_rate_pct = if stats.comparisons == 0 {
        0.0
    } else {
        (stats.static_diffs as f64 / stats.comparisons as f64) * 100.0
    };
    Some(ContextCompilerShadowBlock {
        mode: mode.as_str().to_string(),
        comparisons: stats.comparisons,
        static_diffs: stats.static_diffs,
        full_diffs: stats.full_diffs,
        static_diff_rate_pct,
    })
}

fn shadow_diff_rate_pct(comparisons: u64, diffs: u64) -> f64 {
    if comparisons == 0 {
        0.0
    } else {
        (diffs as f64 / comparisons as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn kernel_shadow_omits_counters_when_not_in_shadow_mode() {
        let config = Config::default();
        let resp = collect_kernel_shadow_stats(&config);
        assert!(resp.policy_shadow.is_none());
        assert!(resp.scheduler_shadow.is_none());
        assert!(resp.context_compiler_shadow.is_none());
    }
}
