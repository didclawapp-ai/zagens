//! Queue gate — **only** via `predicate::*` (Phase 1a.3).

use std::path::Path;
use std::time::Instant;

use anyhow::Result;
use serde_json::Value;

use crate::long_horizon::predicate::{
    self, CompletionGateExec, PredicateContext, PredicateError, PredicateResult, names,
    resolve_tests_pass_command,
};

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

    let mut lines = Vec::new();
    for spec in specs {
        let started = Instant::now();
        let timeout_ms = spec
            .args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(300_000);

        let result = match evaluate_one(workspace, spec, exec, timeout_ms).await {
            Ok(r) => r,
            Err(e) => PredicateResult::fail(
                &spec.predicate,
                "predicate_error",
                e.to_string(),
                ms(started),
                1,
            ),
        };

        lines.push(format!(
            "- `{}`: {} ({}ms)",
            spec.predicate,
            if result.pass { "pass" } else { "fail" },
            result.duration_ms
        ));

        if !result.pass {
            return Ok(GateRunResult {
                passed: false,
                summary: lines.join("\n"),
                failing_predicate: Some(spec.predicate.clone()),
                suggestion: result.suggestion.or(result.code),
            });
        }
    }

    Ok(GateRunResult {
        passed: true,
        summary: lines.join("\n"),
        failing_predicate: None,
        suggestion: None,
    })
}

async fn evaluate_one(
    workspace: &Path,
    spec: &GatePredicateSpec,
    exec: Option<&CompletionGateExec<'_>>,
    timeout_ms: u64,
) -> Result<PredicateResult, PredicateError> {
    if is_sync_only(&spec.predicate) {
        return predicate::evaluate_sync(&spec.predicate, &spec.args, workspace);
    }

    let ctx = PredicateContext {
        workspace,
        timeout_ms,
        exec,
        run_id: format!("queue-gate-{}", spec.predicate),
    };

    match predicate::evaluate(&spec.predicate, &spec.args, &ctx).await {
        Ok(r) => Ok(r),
        Err(PredicateError::NeedsExec(_)) => {
            run_cli_subprocess_gate(workspace, &spec.predicate, &spec.args, timeout_ms).await
        }
        Err(e) => Err(e),
    }
}

fn is_sync_only(name: &str) -> bool {
    matches!(name, names::FILE_EXISTS | names::COMMAND_OUTPUT_MATCHES)
}

async fn run_cli_subprocess_gate(
    workspace: &Path,
    predicate_name: &str,
    args: &Value,
    timeout_ms: u64,
) -> Result<PredicateResult, PredicateError> {
    let started = Instant::now();
    let cmd = args
        .get("cmd")
        .or_else(|| args.get("command"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let cmd = match cmd {
        Some(c) if !c.is_empty() => c,
        _ if predicate_name == names::TESTS_PASS => resolve_tests_pass_command(args)?,
        _ => {
            return Ok(PredicateResult::fail(
                predicate_name,
                "missing_cmd",
                "CLI queue gate needs `cmd` in args when shell exec context is unavailable",
                ms(started),
                1,
            ));
        }
    };

    let _ = timeout_ms;
    let output = tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "sh" })
        .arg(if cfg!(windows) { "/C" } else { "-lc" })
        .arg(&cmd)
        .current_dir(workspace)
        .output()
        .await
        .map_err(|e| PredicateError::InvalidArgs {
            predicate: predicate_name.into(),
            message: e.to_string(),
        })?;

    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if output.status.success() {
        Ok(PredicateResult::pass(predicate_name, ms(started)))
    } else {
        Ok(PredicateResult::fail(
            predicate_name,
            "non_zero_exit",
            if stderr.is_empty() {
                format!("exit code {code}")
            } else {
                stderr
            },
            ms(started),
            code,
        ))
    }
}

fn ms(started: Instant) -> u32 {
    u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
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
