//! Harness verify-loop (Phase 1b.2) — act→verify→retry/rollback skeleton.

use std::future::Future;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::predicate::{
    self, CompletionGateExec, PredicateContext, PredicateError, PredicateResult,
};
use zagens_core::engine::kernel_event::KernelEvent;

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

    /// Verify all stages once (no act/retry). Used by queue gate and unit tests.
    pub async fn verify_stages(&self, stages: &[VerifyStageSpec]) -> HarnessVerifyOutcome {
        let mut records = Vec::new();
        for stage in stages {
            match self.run_one(stage, 0).await {
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
                        retry_no: 0,
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

    /// act→verify loop: `act(retry_no)` runs between verify attempts.
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
            match self.verify_stages(stages).await {
                HarnessVerifyOutcome::Passed { mut records } => {
                    for r in &mut records {
                        r.retry_no = retry_no;
                    }
                    return HarnessVerifyOutcome::Passed { records };
                }
                HarnessVerifyOutcome::Failed { mut records, .. } => {
                    for r in &mut records {
                        r.retry_no = retry_no;
                    }
                    if retry_no < self.config.max_retries {
                        retry_no += 1;
                        continue;
                    }
                    return HarnessVerifyOutcome::Failed {
                        records,
                        exhausted: true,
                        rollback_triggered: self.config.max_retries > 0,
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
        let ctx = PredicateContext {
            workspace: self.workspace,
            timeout_ms: self.config.timeout_ms,
            exec: self.exec,
            run_id: format!("verify-{}-{}", stage.stage, retry_no),
        };

        let result = predicate::evaluate(&stage.predicate, &stage.args, &ctx).await?;
        Ok(record_from_predicate(stage, retry_no, false, &result))
    }
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
}
