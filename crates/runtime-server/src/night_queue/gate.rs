//! Queue gate — **only** via `predicate::*` through `HarnessVerifyLoop` (Phase 1a.3).

use std::path::Path;

use anyhow::Result;

use crate::long_horizon::harness_verify_loop::{
    HarnessVerifyLoop, HarnessVerifyOutcome, VerifyStageSpec,
};
use crate::long_horizon::predicate::CompletionGateExec;

use super::model::GatePredicateSpec;

#[derive(Debug, Clone)]
pub struct GateRunResult {
    pub passed: bool,
    pub summary: String,
    pub failing_predicate: Option<String>,
    pub suggestion: Option<String>,
}

pub async fn run_gate(
    workspace: &Path,
    specs: &[GatePredicateSpec],
    exec: Option<&CompletionGateExec<'_>>,
) -> Result<GateRunResult> {
    if specs.is_empty() {
        return Ok(GateRunResult {
            passed: true,
            summary: "no gate predicates (pass by default)".to_string(),
            failing_predicate: None,
            suggestion: None,
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

    let mut loop_ = HarnessVerifyLoop::new(workspace);
    if let Some(exec) = exec {
        loop_ = loop_.with_exec(exec);
    }

    let outcome = loop_.verify_stages(&stages).await;
    let records = match &outcome {
        HarnessVerifyOutcome::Passed { records } => records,
        HarnessVerifyOutcome::Failed { records, .. } => records,
    };

    let mut lines = Vec::new();
    for record in records {
        lines.push(format!(
            "- `{}`: {} ({}ms)",
            record.predicate,
            if record.pass { "pass" } else { "fail" },
            record.duration_ms
        ));
    }

    match outcome {
        HarnessVerifyOutcome::Passed { .. } => Ok(GateRunResult {
            passed: true,
            summary: lines.join("\n"),
            failing_predicate: None,
            suggestion: None,
        }),
        HarnessVerifyOutcome::Failed { records, .. } => {
            let failing = records.iter().find(|r| !r.pass);
            Ok(GateRunResult {
                passed: false,
                summary: lines.join("\n"),
                failing_predicate: failing.map(|r| r.predicate.clone()),
                suggestion: failing.and_then(|r| r.suggestion.clone().or(r.code.clone())),
            })
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
}
