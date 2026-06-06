//! Completion gate observability events (§6.4 / P0 minimal probe).

use deepseek_core::long_horizon::{CompletionGateVerifyEntry, VerifySource};
use serde::Serialize;

use super::completion_audit::CompletionAuditResult;
use super::manifest_gate::ManifestGateResult;

/// Events queued during gate evaluation; drained by the engine host.
#[derive(Debug, Clone)]
pub enum CompletionGateEvent {
    ManifestGateStart {
        verify_count: u32,
        deliverable_count: u32,
        mode: &'static str,
    },
    ManifestGateResult {
        passed: bool,
        failing_ids: Vec<String>,
        manifest_round: u32,
        payload_json: String,
    },
    CompletionAudit {
        payload_json: String,
    },
    /// Generic stub / incompleteness scan result (§ stub gate). `payload_json`
    /// carries the mode + blocking/todo/total counts + a small sample.
    StubGate {
        payload_json: String,
    },
    /// Strict-mode plan-bootstrap gate: `nudged=true` when a "establish a plan"
    /// nudge was injected (empty graph + real work), `false` when rounds were
    /// exhausted and the gate gave up. `round` is the session plan-gate count.
    PlanGate {
        nudged: bool,
        round: u32,
    },
    /// P1-4: cross-layer integration observe heuristics (Electron→Tauri class).
    IntegrationGate {
        payload_json: String,
    },
}

#[derive(Serialize)]
struct SourceCounts {
    operator: u32,
    model_declared: u32,
    toolchain: u32,
}

#[derive(Serialize)]
struct ManifestGateResultPayload<'a> {
    passed: bool,
    failing_ids: &'a [String],
    manifest_round: u32,
    enforced_failing: u32,
    observed_failing: u32,
    sources: SourceCounts,
    results: &'a [super::manifest_gate::VerifyRunResult],
}

impl CompletionGateEvent {
    #[must_use]
    pub fn status_message(&self) -> String {
        match self {
            Self::ManifestGateStart {
                verify_count,
                deliverable_count,
                mode,
            } => format!(
                "long_horizon.manifest_gate_start: {{\"verify_count\":{verify_count},\"deliverable_count\":{deliverable_count},\"mode\":\"{mode}\"}}"
            ),
            Self::ManifestGateResult {
                passed,
                failing_ids,
                manifest_round,
                payload_json,
            } => format!(
                "long_horizon.manifest_gate_result: {{\"passed\":{passed},\"failing_count\":{},\"manifest_round\":{manifest_round},\"detail\":{payload_json}}}",
                failing_ids.len()
            ),
            Self::CompletionAudit { payload_json } => {
                format!("long_horizon.completion_audit: {payload_json}")
            }
            Self::StubGate { payload_json } => {
                format!("long_horizon.stub_gate: {payload_json}")
            }
            Self::PlanGate { nudged, round } => {
                format!("long_horizon.plan_gate: {{\"nudged\":{nudged},\"round\":{round}}}")
            }
            Self::IntegrationGate { payload_json } => {
                format!("long_horizon.integration_gate: {payload_json}")
            }
        }
    }

    #[must_use]
    pub fn plan_gate(nudged: bool, round: u32) -> Self {
        Self::PlanGate { nudged, round }
    }

    pub fn queue_manifest_start(
        out: &mut Vec<Self>,
        verify_count: u32,
        deliverable_count: u32,
        mode: deepseek_core::long_horizon::CompletionGateMode,
    ) {
        let mode_str = match mode {
            deepseek_core::long_horizon::CompletionGateMode::Enforce => "enforce",
            deepseek_core::long_horizon::CompletionGateMode::Observe => "observe",
        };
        out.push(Self::ManifestGateStart {
            verify_count,
            deliverable_count,
            mode: mode_str,
        });
    }

    pub fn queue_manifest_result(
        out: &mut Vec<Self>,
        result: &ManifestGateResult,
        entries: &[CompletionGateVerifyEntry],
        manifest_round: u32,
        enforced_failing: u32,
        observed_failing: u32,
    ) {
        let mut sources = SourceCounts {
            operator: 0,
            model_declared: 0,
            toolchain: 0,
        };
        for e in entries {
            match e.source {
                VerifySource::Operator => sources.operator += 1,
                VerifySource::ModelDeclared => sources.model_declared += 1,
                VerifySource::Toolchain => sources.toolchain += 1,
            }
        }
        let payload = ManifestGateResultPayload {
            passed: result.passed,
            failing_ids: &result.failing_ids,
            manifest_round,
            enforced_failing,
            observed_failing,
            sources,
            results: &result.results,
        };
        let payload_json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
        out.push(Self::ManifestGateResult {
            passed: result.passed,
            failing_ids: result.failing_ids.clone(),
            manifest_round,
            payload_json,
        });
    }

    pub fn queue_completion_audit(out: &mut Vec<Self>, audit: &CompletionAuditResult) {
        let payload_json = serde_json::to_string(audit).unwrap_or_else(|_| "{}".to_string());
        out.push(Self::CompletionAudit { payload_json });
    }

    pub fn queue_integration_observe(out: &mut Vec<Self>, gaps: &[String], enforce: bool) {
        let payload = if enforce {
            serde_json::json!({
                "enforce": true,
                "gap_count": gaps.len(),
                "gaps": gaps,
            })
        } else {
            serde_json::json!({
                "observe": true,
                "gap_count": gaps.len(),
                "gaps": gaps,
            })
        };
        out.push(Self::IntegrationGate {
            payload_json: payload.to_string(),
        });
    }
}
