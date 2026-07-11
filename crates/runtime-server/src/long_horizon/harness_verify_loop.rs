//! Harness verify-loop (Phase 1b.2) — act→verify→retry/rollback skeleton.

use std::future::Future;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::predicate::{
    self, CompletionGateExec, PredicateContext, PredicateError, PredicateResult, VerifyRunResult,
    names,
};
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::long_horizon::CompletionGateVerifyEntry;

/// One verify stage in a loop (maps to `harness_verify` telemetry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyStageSpec {
    pub stage: String,
    pub predicate: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessVerifyRecord {
    pub stage: String,
    pub predicate: String,
    pub pass: bool,
    pub retry_no: u32,
    pub rollback_triggered: bool,
    pub duration_ms: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HarnessVerifyLoopConfig {
    pub max_retries: u32,
    pub timeout_ms: u64,
}

impl Default for HarnessVerifyLoopConfig {
    fn default() -> Self {
        Self {
            max_retries: 2,
            timeout_ms: 300_000,
        }
    }
}

#[derive(Debug, Clone)]
pub enum HarnessVerifyOutcome {
    Passed {
        records: Vec<HarnessVerifyRecord>,
    },
    Failed {
        records: Vec<HarnessVerifyRecord>,
        exhausted: bool,
        rollback_triggered: bool,
    },
}

pub struct HarnessVerifyLoop<'a> {
    pub workspace: &'a Path,
    pub exec: Option<&'a CompletionGateExec<'a>>,
    pub config: HarnessVerifyLoopConfig,
}

impl<'a> HarnessVerifyLoop<'a> {
    #[must_use]
    pub fn new(workspace: &'a Path) -> Self {
        Self {
            workspace,
            exec: None,
            config: HarnessVerifyLoopConfig::default(),
        }
    }

    #[must_use]
    pub fn with_exec(mut self, exec: &'a CompletionGateExec<'a>) -> Self {
        self.exec = Some(exec);
        self
    }

    #[must_use]
    pub fn with_config(mut self, config: HarnessVerifyLoopConfig) -> Self {
        self.config = config;
        self
    }

    /// Verify all stages once (no act/retry). Used by unit tests and single-shot callers.
    pub async fn verify_stages(&self, stages: &[VerifyStageSpec]) -> HarnessVerifyOutcome {
        self.verify_stages_at(stages, 0).await
    }

    /// Verify all stages at a fixed `retry_no` (no act).
    pub async fn verify_stages_at(
        &self,
        stages: &[VerifyStageSpec],
        retry_no: u32,
    ) -> HarnessVerifyOutcome {
        let mut records = Vec::new();
        for stage in stages {
            match self.run_one(stage, retry_no).await {
                Ok(rec) if rec.pass => records.push(rec),
                Ok(rec) => {
                    records.push(rec);
                    return HarnessVerifyOutcome::Failed {
                        records,
                        exhausted: false,
                        rollback_triggered: false,
                    };
                }
                Err(e) => {
                    records.push(HarnessVerifyRecord {
                        stage: stage.stage.clone(),
                        predicate: stage.predicate.clone(),
                        pass: false,
                        retry_no,
                        rollback_triggered: false,
                        duration_ms: 0,
                        code: Some("predicate_error".into()),
                        suggestion: Some(e.to_string()),
                    });
                    return HarnessVerifyOutcome::Failed {
                        records,
                        exhausted: false,
                        rollback_triggered: false,
                    };
                }
            }
        }
        HarnessVerifyOutcome::Passed { records }
    }

    /// act→verify loop: `act(retry_no)` runs before each verify attempt.
    ///
    /// On exhausted failure, sets `rollback_triggered` on the outcome and on
    /// every record when `max_retries > 0` (HL-1 / HL-3).
    pub async fn run_with_act<F, Fut>(
        &self,
        stages: &[VerifyStageSpec],
        mut act: F,
    ) -> HarnessVerifyOutcome
    where
        F: FnMut(u32) -> Fut,
        Fut: Future<Output = ()>,
    {
        let mut retry_no = 0u32;
        loop {
            act(retry_no).await;
            match self.verify_stages_at(stages, retry_no).await {
                HarnessVerifyOutcome::Passed { records } => {
                    return HarnessVerifyOutcome::Passed { records };
                }
                HarnessVerifyOutcome::Failed { records, .. } => {
                    if retry_no < self.config.max_retries {
                        retry_no += 1;
                        continue;
                    }
                    let rollback = self.config.max_retries > 0;
                    let records = if rollback {
                        mark_records_rollback(records)
                    } else {
                        records
                    };
                    return HarnessVerifyOutcome::Failed {
                        records,
                        exhausted: true,
                        rollback_triggered: rollback,
                    };
                }
            }
        }
    }

    async fn run_one(
        &self,
        stage: &VerifyStageSpec,
        retry_no: u32,
    ) -> Result<HarnessVerifyRecord, PredicateError> {
        let timeout_ms = stage
            .args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.config.timeout_ms);
        let ctx = PredicateContext {
            workspace: self.workspace,
            timeout_ms,
            exec: self.exec,
            run_id: format!("verify-{}-{}", stage.stage, retry_no),
        };

        let result = if is_sync_only(&stage.predicate) {
            predicate::evaluate_sync(&stage.predicate, &stage.args, self.workspace)?
        } else {
            match predicate::evaluate(&stage.predicate, &stage.args, &ctx).await {
                Ok(r) => r,
                Err(PredicateError::NeedsExec(_)) if self.exec.is_none() => {
                    run_cli_subprocess_predicate(
                        self.workspace,
                        &stage.predicate,
                        &stage.args,
                        timeout_ms,
                    )
                    .await?
                }
                Err(e) => return Err(e),
            }
        };
        Ok(record_from_predicate(stage, retry_no, false, &result))
    }
}

fn is_sync_only(name: &str) -> bool {
    matches!(name, names::FILE_EXISTS | names::COMMAND_OUTPUT_MATCHES)
}

async fn run_cli_subprocess_predicate(
    workspace: &Path,
    predicate_name: &str,
    args: &serde_json::Value,
    timeout_ms: u64,
) -> Result<PredicateResult, PredicateError> {
    use std::time::Instant;

    let started = Instant::now();
    let cmd = args
        .get("cmd")
        .or_else(|| args.get("command"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let cmd = match cmd {
        Some(c) if !c.is_empty() => c,
        _ if predicate_name == names::TESTS_PASS => predicate::resolve_tests_pass_command(args)?,
        _ => {
            return Ok(PredicateResult::fail(
                predicate_name,
                "missing_cmd",
                "CLI gate needs `cmd` in args when shell exec context is unavailable",
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

fn ms(started: std::time::Instant) -> u32 {
    u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

#[must_use]
pub fn mark_records_rollback(mut records: Vec<HarnessVerifyRecord>) -> Vec<HarnessVerifyRecord> {
    for r in &mut records {
        r.rollback_triggered = true;
    }
    records
}

/// Collect records from either outcome variant.
#[must_use]
pub fn outcome_records(outcome: &HarnessVerifyOutcome) -> &[HarnessVerifyRecord] {
    match outcome {
        HarnessVerifyOutcome::Passed { records } | HarnessVerifyOutcome::Failed { records, .. } => {
            records
        }
    }
}

/// Map layer-2 manifest verify runs to harness telemetry (no re-exec).
#[must_use]
pub fn records_from_manifest_gate(
    entries: &[CompletionGateVerifyEntry],
    results: &[VerifyRunResult],
    retry_no: u32,
) -> Vec<HarnessVerifyRecord> {
    entries
        .iter()
        .zip(results.iter())
        .map(|(entry, run)| record_from_manifest_verify(entry, run, retry_no))
        .collect()
}

#[must_use]
pub fn record_from_manifest_verify(
    entry: &CompletionGateVerifyEntry,
    run: &VerifyRunResult,
    retry_no: u32,
) -> HarnessVerifyRecord {
    let pass = run.pass();
    let (code, suggestion) = if pass {
        (None, None)
    } else {
        let code = match run.exit_class {
            super::predicate::VerifyExitClass::Infra => "infra",
            super::predicate::VerifyExitClass::Timeout => "timeout",
            super::predicate::VerifyExitClass::Cancelled => "cancelled",
            super::predicate::VerifyExitClass::Assertion => "assertion",
            super::predicate::VerifyExitClass::Ok => "non_zero_exit",
        };
        let suggestion = if run.stderr_tail.is_empty() {
            format!("exit code {}", run.exit_code)
        } else {
            run.stderr_tail.clone()
        };
        (Some(code.into()), Some(suggestion))
    };
    HarnessVerifyRecord {
        stage: entry.id.clone(),
        predicate: names::EXIT_CODE.into(),
        pass,
        retry_no,
        rollback_triggered: false,
        duration_ms: 0,
        code,
        suggestion,
    }
}

/// Sidecar status line (double-write companion to `KernelEvent::HarnessVerify`).
#[must_use]
pub fn harness_verify_status_message(record: &HarnessVerifyRecord) -> String {
    let code = record
        .code
        .as_deref()
        .map(|c| format!(",\"code\":\"{c}\""))
        .unwrap_or_default();
    let suggestion = record
        .suggestion
        .as_ref()
        .map(|s| {
            let esc = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!(",\"suggestion\":\"{esc}\"")
        })
        .unwrap_or_default();
    format!(
        "long_horizon.harness_verify: {{\"stage\":\"{}\",\"predicate\":\"{}\",\"pass\":{},\"retry_no\":{},\"rollback_triggered\":{},\"duration_ms\":{}{code}{suggestion}}}",
        record.stage,
        record.predicate,
        record.pass,
        record.retry_no,
        record.rollback_triggered,
        record.duration_ms,
    )
}

#[must_use]
pub fn record_to_kernel_event(
    turn_id: impl Into<String>,
    record: &HarnessVerifyRecord,
) -> KernelEvent {
    KernelEvent::HarnessVerify {
        turn_id: turn_id.into(),
        stage: record.stage.clone(),
        predicate: record.predicate.clone(),
        pass: record.pass,
        retry_no: record.retry_no,
        rollback_triggered: record.rollback_triggered,
        duration_ms: record.duration_ms,
        code: record.code.clone(),
        suggestion: record.suggestion.clone(),
    }
}

#[must_use]
pub fn record_from_predicate(
    stage: &VerifyStageSpec,
    retry_no: u32,
    rollback_triggered: bool,
    result: &PredicateResult,
) -> HarnessVerifyRecord {
    HarnessVerifyRecord {
        stage: stage.stage.clone(),
        predicate: stage.predicate.clone(),
        pass: result.pass,
        retry_no,
        rollback_triggered,
        duration_ms: result.duration_ms,
        code: result.code.clone(),
        suggestion: result.suggestion.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::long_horizon::predicate::names;
    use serde_json::json;
    use tempfile::TempDir;

    #[tokio::test]
    async fn verify_stages_file_exists_passes() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("ok.txt"), b"1").unwrap();
        let loop_ = HarnessVerifyLoop::new(dir.path());
        let outcome = loop_
            .verify_stages(&[VerifyStageSpec {
                stage: "check".into(),
                predicate: names::FILE_EXISTS.into(),
                args: json!({"path": "ok.txt"}),
            }])
            .await;
        assert!(matches!(outcome, HarnessVerifyOutcome::Passed { .. }));
    }

    /// HL-1: act repairs missing file on retry → pass with retry_no == 1.
    #[tokio::test]
    async fn run_with_act_heals_on_retry() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("heal.txt");
        let loop_ = HarnessVerifyLoop::new(dir.path()).with_config(HarnessVerifyLoopConfig {
            max_retries: 2,
            timeout_ms: 5_000,
        });
        let stages = [VerifyStageSpec {
            stage: "heal".into(),
            predicate: names::FILE_EXISTS.into(),
            args: json!({"path": "heal.txt"}),
        }];
        let outcome = loop_
            .run_with_act(&stages, |retry_no| {
                let target = target.clone();
                async move {
                    if retry_no >= 1 {
                        let _ = std::fs::write(&target, b"healed");
                    }
                }
            })
            .await;
        match outcome {
            HarnessVerifyOutcome::Passed { records } => {
                assert_eq!(records.len(), 1);
                assert!(records[0].pass);
                assert_eq!(records[0].retry_no, 1);
                assert!(!records[0].rollback_triggered);
            }
            other => panic!("expected pass after heal, got {other:?}"),
        }
    }

    /// HL-3: exhausted retries mark rollback_triggered on records.
    #[tokio::test]
    async fn run_with_act_exhausted_marks_rollback() {
        let dir = TempDir::new().unwrap();
        let loop_ = HarnessVerifyLoop::new(dir.path()).with_config(HarnessVerifyLoopConfig {
            max_retries: 1,
            timeout_ms: 5_000,
        });
        let outcome = loop_
            .run_with_act(
                &[VerifyStageSpec {
                    stage: "missing".into(),
                    predicate: names::FILE_EXISTS.into(),
                    args: json!({"path": "never.txt"}),
                }],
                |_| async {},
            )
            .await;
        match outcome {
            HarnessVerifyOutcome::Failed {
                records,
                exhausted,
                rollback_triggered,
            } => {
                assert!(exhausted);
                assert!(rollback_triggered);
                assert!(records.iter().all(|r| r.rollback_triggered));
                assert_eq!(records.last().map(|r| r.retry_no), Some(1));
            }
            other => panic!("expected exhausted fail, got {other:?}"),
        }
    }

    #[test]
    fn record_from_manifest_verify_maps_failure() {
        use super::predicate::VerifyExitClass;
        use zagens_core::long_horizon::{CompletionGateVerifyEntry, ManifestShell, VerifySource};

        let entry = CompletionGateVerifyEntry {
            id: "build".into(),
            cmd: Some("cargo test".into()),
            argv: Vec::new(),
            shell: ManifestShell::Default,
            timeout_secs: 60,
            source: VerifySource::Operator,
        };
        let run = super::predicate::VerifyRunResult {
            id: "build".into(),
            command_display: "cargo test".into(),
            exit_code: 1,
            exit_class: VerifyExitClass::Assertion,
            stdout_tail: String::new(),
            stderr_tail: "assertion failed".into(),
        };
        let record = record_from_manifest_verify(&entry, &run, 1);
        assert!(!record.pass);
        assert_eq!(record.stage, "build");
        assert_eq!(record.predicate, names::EXIT_CODE);
        assert_eq!(record.retry_no, 1);
        assert_eq!(record.code.as_deref(), Some("assertion"));
    }

    #[test]
    fn harness_verify_status_message_is_jsonish() {
        let msg = harness_verify_status_message(&HarnessVerifyRecord {
            stage: "build".into(),
            predicate: names::EXIT_CODE.into(),
            pass: true,
            retry_no: 0,
            rollback_triggered: false,
            duration_ms: 12,
            code: None,
            suggestion: None,
        });
        assert!(msg.starts_with("long_horizon.harness_verify:"));
        assert!(msg.contains("\"pass\":true"));
    }

    /// Phase 1b.4: completion gate must queue harness verify telemetry after manifest run.
    #[test]
    fn completion_gate_flow_queues_harness_verify() {
        let src = include_str!("completion_gate_flow.rs");
        assert!(src.contains("records_from_manifest_gate"));
        assert!(src.contains("pending_harness_verify"));
    }

    /// Phase 1b.4: queue gate routes through verify-loop, not manifest runner.
    #[test]
    fn queue_gate_routes_through_verify_loop() {
        let src = include_str!("../night_queue/gate.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap_or(src);
        assert!(
            !prod.contains("run_manifest_gate("),
            "queue gate must not invoke manifest gate runner directly"
        );
        assert!(
            prod.contains("HarnessVerifyLoop"),
            "queue gate must route through HarnessVerifyLoop"
        );
        assert!(
            prod.contains("run_with_act"),
            "HL-1: queue gate must call run_with_act"
        );
    }

    /// HL-1: stage gate try_pass_stage must call run_with_act.
    #[test]
    fn stage_gate_try_pass_uses_run_with_act() {
        let src = include_str!("stage_gate.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap_or(src);
        assert!(
            prod.contains("run_with_act"),
            "HL-1/HL-6: stage gate must call run_with_act"
        );
    }
}
