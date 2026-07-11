//! Queue gate — **only** via `predicate::*` through `HarnessVerifyLoop` (Phase 1a.3).
//!
//! HL-1: production path calls [`HarnessVerifyLoop::run_with_act`].

use std::path::Path;

use anyhow::Result;

use crate::long_horizon::harness_verify_loop::{
    HarnessVerifyLoop, HarnessVerifyLoopConfig, HarnessVerifyOutcome, HarnessVerifyRecord,
    VerifyStageSpec, outcome_records,
};
use crate::long_horizon::predicate::CompletionGateExec;

use super::model::GatePredicateSpec;

#[derive(Debug, Clone)]
pub struct GateRunResult {
    pub passed: bool,
    pub summary: String,
    pub failing_predicate: Option<String>,
    pub suggestion: Option<String>,
    pub records: Vec<HarnessVerifyRecord>,
    pub exhausted: bool,
    pub rollback_triggered: bool,
}

pub async fn run_gate(
    workspace: &Path,
    specs: &[GatePredicateSpec],
    exec: Option<&CompletionGateExec<'_>>,
) -> Result<GateRunResult> {
    run_gate_with_act(workspace, specs, exec, |_| async {}).await
}

/// Queue gate with optional repair `act` between verify retries (HL-1).
pub async fn run_gate_with_act<F, Fut>(
    workspace: &Path,
    specs: &[GatePredicateSpec],
    exec: Option<&CompletionGateExec<'_>>,
    act: F,
) -> Result<GateRunResult>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    if specs.is_empty() {
        return Ok(GateRunResult {
            passed: true,
            summary: "no gate predicates (pass by default)".to_string(),
            failing_predicate: None,
            suggestion: None,
            records: Vec::new(),
            exhausted: false,
            rollback_triggered: false,
        });
    }

    let stages: Vec<VerifyStageSpec> = specs
        .iter()
        .enumerate()
        .map(|(i, spec)| VerifyStageSpec {
            stage: format!("queue-gate-{i}"),
            predicate: spec.predicate.clone(),
            args: spec.args.clone(),
        })
        .collect();

    // Single-shot by default (agent already acted). Callers that supply a
    // repair `act` should use `run_gate_with_retries`.
    let mut loop_ = HarnessVerifyLoop::new(workspace).with_config(HarnessVerifyLoopConfig {
        max_retries: 0,
        timeout_ms: 300_000,
    });
    if let Some(exec) = exec {
        loop_ = loop_.with_exec(exec);
    }

    let outcome = loop_.run_with_act(&stages, act).await;
    Ok(gate_result_from_outcome(outcome))
}

/// Like [`run_gate_with_act`] but honors `max_retries` for heal/retry demos (HL-1).
pub async fn run_gate_with_retries<F, Fut>(
    workspace: &Path,
    specs: &[GatePredicateSpec],
    exec: Option<&CompletionGateExec<'_>>,
    max_retries: u32,
    act: F,
) -> Result<GateRunResult>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    if specs.is_empty() {
        return run_gate(workspace, specs, exec).await;
    }

    let stages: Vec<VerifyStageSpec> = specs
        .iter()
        .enumerate()
        .map(|(i, spec)| VerifyStageSpec {
            stage: format!("queue-gate-{i}"),
            predicate: spec.predicate.clone(),
            args: spec.args.clone(),
        })
        .collect();

    let mut loop_ = HarnessVerifyLoop::new(workspace).with_config(HarnessVerifyLoopConfig {
        max_retries,
        timeout_ms: 300_000,
    });
    if let Some(exec) = exec {
        loop_ = loop_.with_exec(exec);
    }

    let outcome = loop_.run_with_act(&stages, act).await;
    Ok(gate_result_from_outcome(outcome))
}

fn gate_result_from_outcome(outcome: HarnessVerifyOutcome) -> GateRunResult {
    let records = outcome_records(&outcome).to_vec();
    let mut lines = Vec::new();
    for record in &records {
        lines.push(format!(
            "- `{}`: {} (retry={}, {}ms)",
            record.predicate,
            if record.pass { "pass" } else { "fail" },
            record.retry_no,
            record.duration_ms
        ));
    }

    match outcome {
        HarnessVerifyOutcome::Passed { .. } => GateRunResult {
            passed: true,
            summary: lines.join("\n"),
            failing_predicate: None,
            suggestion: None,
            records,
            exhausted: false,
            rollback_triggered: false,
        },
        HarnessVerifyOutcome::Failed {
            exhausted,
            rollback_triggered,
            ..
        } => {
            let failing = records.iter().find(|r| !r.pass);
            GateRunResult {
                passed: false,
                summary: lines.join("\n"),
                failing_predicate: failing.map(|r| r.predicate.clone()),
                suggestion: failing.and_then(|r| r.suggestion.clone().or(r.code.clone())),
                records,
                exhausted,
                rollback_triggered,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::long_horizon::predicate::names;
    use serde_json::json;
    use tempfile::TempDir;

    #[tokio::test]
    async fn gate_file_exists_passes() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("x"), b"1").unwrap();
        let res = run_gate(
            dir.path(),
            &[GatePredicateSpec {
                predicate: names::FILE_EXISTS.into(),
                args: json!({"path": "x"}),
            }],
            None,
        )
        .await
        .unwrap();
        assert!(res.passed);
        assert!(!res.records.is_empty());
    }

    #[tokio::test]
    async fn gate_tests_pass_delegates_to_exit_code() {
        let dir = TempDir::new().unwrap();
        let cmd = if cfg!(windows) {
            "cmd /c exit 0"
        } else {
            "true"
        };
        let res = run_gate(
            dir.path(),
            &[GatePredicateSpec {
                predicate: names::TESTS_PASS.into(),
                args: json!({"cmd": cmd}),
            }],
            None,
        )
        .await
        .unwrap();
        assert!(res.passed, "{}", res.summary);
    }

    #[tokio::test]
    async fn gate_with_retries_heals_via_act() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("queue-heal.txt");
        let res = run_gate_with_retries(
            dir.path(),
            &[GatePredicateSpec {
                predicate: names::FILE_EXISTS.into(),
                args: json!({"path": "queue-heal.txt"}),
            }],
            None,
            2,
            |retry_no| {
                let target = target.clone();
                async move {
                    if retry_no >= 1 {
                        let _ = std::fs::write(&target, b"ok");
                    }
                }
            },
        )
        .await
        .unwrap();
        assert!(res.passed, "{}", res.summary);
        assert_eq!(res.records.last().map(|r| r.retry_no), Some(1));
    }
}
