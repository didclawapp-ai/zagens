//! Skill stage gate (Phase 2a.2) — tool exposure + execution fallback.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use zagens_core::long_horizon::{HarnessContract, STAGE_GATE_ALWAYS_ALLOWED};

use super::harness_verify_loop::HarnessVerifyLoop;
use super::predicate::{self, CompletionGateExec};

pub const HARNESS_MANIFEST_FILENAME: &str = "harness.toml";

/// Session-scoped stage gate state (stored in `LongHorizonSessionState`).
#[derive(Debug, Clone, Default)]
pub struct StageGateSession {
    pub contract: Option<HarnessContract>,
    pub verified_stages: HashSet<String>,
    pub enforce: bool,
}

#[derive(Debug, Clone)]
pub struct StageGateBlocked {
    pub skill: String,
    pub stage: String,
    pub tool_name: String,
    pub suggestion: String,
}

impl StageGateBlocked {
    pub const CODE: &'static str = "stage_gate_blocked";

    #[must_use]
    pub fn code(&self) -> &'static str {
        Self::CODE
    }

    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "Stage gate blocked: tool `{tool}` is not available in stage `{stage}` for skill `{skill}`. {hint}",
            tool = self.tool_name,
            stage = self.stage,
            skill = self.skill,
            hint = self.suggestion
        )
    }
}

impl StageGateSession {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.enforce && self.contract.as_ref().is_some_and(|c| c.is_active())
    }

    pub fn load_contract(&mut self, contract: HarnessContract, enforce: bool) {
        self.verified_stages.clear();
        self.contract = Some(contract);
        self.enforce = enforce;
    }

    pub fn load_manifest_file(&mut self, path: &Path, enforce: bool) -> Result<(), String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("read harness manifest {}: {e}", path.display()))?;
        let contract = HarnessContract::parse_toml(&raw)
            .map_err(|e| format!("parse harness manifest {}: {e}", path.display()))?;
        self.load_contract(contract, enforce);
        Ok(())
    }

    #[must_use]
    pub fn current_stage_id(&self) -> Option<String> {
        let contract = self.contract.as_ref()?;
        let verified: Vec<String> = self.verified_stages.iter().cloned().collect();
        contract.current_stage_id(&verified)
    }

    #[must_use]
    pub fn allowed_tool_names(&self) -> Option<Vec<String>> {
        if !self.is_active() {
            return None;
        }
        let contract = self.contract.as_ref()?;
        let verified: Vec<String> = self.verified_stages.iter().cloned().collect();
        Some(contract.allowed_tools(&verified))
    }

    pub fn check_tool(&self, tool_name: &str) -> Result<(), StageGateBlocked> {
        if !self.is_active() {
            return Ok(());
        }
        let contract = self.contract.as_ref().expect("active implies contract");
        let verified: Vec<String> = self.verified_stages.iter().cloned().collect();
        if contract.tool_allowed(tool_name, &verified) {
            return Ok(());
        }
        let stage = self
            .current_stage_id()
            .unwrap_or_else(|| "complete".to_string());
        Err(StageGateBlocked {
            skill: contract.harness.id.clone(),
            stage,
            tool_name: tool_name.to_string(),
            suggestion: format!(
                "Complete verify for the current stage before calling `{tool}`. Use assert_* tools to run stage verify predicates.",
                tool = tool_name
            ),
        })
    }

    pub fn filter_tool_catalog<T>(&self, tools: Vec<T>, name: impl Fn(&T) -> &str) -> Vec<T> {
        let Some(allowed) = self.allowed_tool_names() else {
            return tools;
        };
        let allow: HashSet<&str> = allowed.iter().map(String::as_str).collect();
        tools
            .into_iter()
            .filter(|t| {
                let n = name(t);
                allow.contains(n) || STAGE_GATE_ALWAYS_ALLOWED.contains(&n)
            })
            .collect()
    }

    /// Run all verify predicates for `stage_id`; on full pass mark stage verified.
    ///
    /// Uses [`HarnessVerifyLoop::run_with_act`] (HL-1) so retries honor
    /// `verify_budget.max_retries`. Returns verify records for telemetry (HL-2).
    pub async fn try_pass_stage(
        &mut self,
        workspace: &Path,
        stage_id: &str,
        exec: Option<&CompletionGateExec<'_>>,
    ) -> Result<
        (bool, Vec<super::harness_verify_loop::HarnessVerifyRecord>),
        predicate::PredicateError,
    > {
        let Some(contract) = self.contract.clone() else {
            return Ok((false, Vec::new()));
        };
        let entries = contract.verify_for_stage(stage_id);
        if entries.is_empty() {
            self.verified_stages.insert(stage_id.to_string());
            return Ok((true, Vec::new()));
        }

        let stages: Vec<super::harness_verify_loop::VerifyStageSpec> = entries
            .into_iter()
            .map(|entry| super::harness_verify_loop::VerifyStageSpec {
                stage: stage_id.to_string(),
                predicate: entry.predicate.clone(),
                args: entry.args.clone(),
            })
            .collect();

        let mut loop_ = HarnessVerifyLoop::new(workspace).with_config(
            super::harness_verify_loop::HarnessVerifyLoopConfig {
                max_retries: contract.verify_budget.max_retries,
                timeout_ms: contract.verify_budget.timeout_ms,
            },
        );
        if let Some(exec) = exec {
            loop_ = loop_.with_exec(exec);
        }

        let outcome = loop_.run_with_act(&stages, |_| async {}).await;
        let records = super::harness_verify_loop::outcome_records(&outcome).to_vec();
        match outcome {
            super::harness_verify_loop::HarnessVerifyOutcome::Passed { .. } => {
                self.verified_stages.insert(stage_id.to_string());
                Ok((true, records))
            }
            super::harness_verify_loop::HarnessVerifyOutcome::Failed { .. } => Ok((false, records)),
        }
    }

    /// Flat contract verify rows via `HarnessVerifyLoop::run_with_act` (HL-1 / HL-6).
    pub async fn run_flat_verify(
        &self,
        workspace: &Path,
        exec: Option<&CompletionGateExec<'_>>,
    ) -> super::harness_verify_loop::HarnessVerifyOutcome {
        self.run_flat_verify_with_act(workspace, exec, |_| async {})
            .await
    }

    /// Flat verify with a caller-supplied repair `act` between retries.
    pub async fn run_flat_verify_with_act<F, Fut>(
        &self,
        workspace: &Path,
        exec: Option<&CompletionGateExec<'_>>,
        act: F,
    ) -> super::harness_verify_loop::HarnessVerifyOutcome
    where
        F: FnMut(u32) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let Some(contract) = self.contract.as_ref() else {
            return super::harness_verify_loop::HarnessVerifyOutcome::Passed {
                records: Vec::new(),
            };
        };
        let stages: Vec<super::harness_verify_loop::VerifyStageSpec> = contract
            .verify_stages()
            .into_iter()
            .map(|row| super::harness_verify_loop::VerifyStageSpec {
                stage: row.stage,
                predicate: row.predicate,
                args: row.args,
            })
            .collect();
        let mut loop_ = HarnessVerifyLoop::new(workspace).with_config(
            super::harness_verify_loop::HarnessVerifyLoopConfig {
                max_retries: contract.verify_budget.max_retries,
                timeout_ms: contract.verify_budget.timeout_ms,
            },
        );
        if let Some(exec) = exec {
            loop_ = loop_.with_exec(exec);
        }
        loop_.run_with_act(&stages, act).await
    }
}

/// Resolve `harness.toml` next to a loaded `SKILL.md`.
#[must_use]
pub fn manifest_path_for_skill(skill_md: &Path) -> PathBuf {
    skill_md
        .parent()
        .map(|dir| dir.join(HARNESS_MANIFEST_FILENAME))
        .unwrap_or_else(|| PathBuf::from(HARNESS_MANIFEST_FILENAME))
}

#[must_use]
pub fn blocked_to_kernel_event(
    turn_id: impl Into<String>,
    step_idx: u32,
    blocked: &StageGateBlocked,
) -> zagens_core::engine::kernel_event::KernelEvent {
    use zagens_core::engine::kernel_event::KernelEvent;
    KernelEvent::StageGateBlocked {
        turn_id: turn_id.into(),
        step_idx,
        skill: blocked.skill.clone(),
        stage: blocked.stage.clone(),
        tool_name: blocked.tool_name.clone(),
        code: StageGateBlocked::CODE.to_string(),
        suggestion: blocked.suggestion.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;
    use zagens_core::long_horizon::{HarnessContract, StageSpec, VerifyEntry};

    fn sample_contract() -> HarnessContract {
        HarnessContract {
            harness: zagens_core::long_horizon::HarnessMeta {
                id: "test-skill".into(),
                ..Default::default()
            },
            stages: vec![
                StageSpec {
                    id: "a".into(),
                    tools: vec!["read_file".into()],
                    requires: vec![],
                },
                StageSpec {
                    id: "b".into(),
                    tools: vec!["write_file".into()],
                    requires: vec!["a".into()],
                },
            ],
            verify: vec![VerifyEntry {
                stage: Some("a".into()),
                id: None,
                predicate: "file_exists".into(),
                args: json!({"path": "x.txt"}),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn blocks_tool_outside_stage() {
        let mut session = StageGateSession::default();
        session.load_contract(sample_contract(), true);
        let err = session.check_tool("write_file").unwrap_err();
        assert_eq!(err.code(), StageGateBlocked::CODE);
        assert_eq!(err.tool_name, "write_file");
    }

    #[tokio::test]
    async fn stage_verify_pass_marks_verified() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("x.txt"), b"1").unwrap();
        let mut session = StageGateSession::default();
        session.load_contract(sample_contract(), true);
        let (ok, records) = session.try_pass_stage(dir.path(), "a", None).await.unwrap();
        assert!(ok);
        assert!(!records.is_empty());
        assert!(session.verified_stages.contains("a"));
        assert!(session.check_tool("write_file").is_ok());
    }

    const OFFICE_PILOT_HARNESS: &str =
        include_str!("../../assets/skills/office-weekly-report/harness.toml");

    fn office_pilot_session() -> StageGateSession {
        let mut session = StageGateSession::default();
        let contract = HarnessContract::parse_toml(OFFICE_PILOT_HARNESS).expect("office pilot");
        session.load_contract(contract, true);
        session
    }

    #[test]
    fn office_pilot_blocks_write_before_prepare() {
        let session = office_pilot_session();
        assert!(session.check_tool("read_office").is_ok());
        assert!(session.check_tool("write_office").is_err());
        assert!(session.check_tool("write_file").is_err());
    }

    #[tokio::test]
    async fn office_pilot_write_then_readback_blocks_rewrite_bypass() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("deliverables")).unwrap();
        std::fs::write(dir.path().join("deliverables/weekly.docx"), b"pk").unwrap();

        let mut session = office_pilot_session();
        assert!(
            session
                .try_pass_stage(dir.path(), "prepare", None)
                .await
                .unwrap()
                .0
        );
        assert!(session.check_tool("write_office").is_ok());

        assert!(
            session
                .try_pass_stage(dir.path(), "write", None)
                .await
                .unwrap()
                .0
        );
        assert_eq!(
            session.current_stage_id().as_deref(),
            Some("readback_verify")
        );
        assert!(session.check_tool("read_office").is_ok());
        assert!(
            session.check_tool("write_office").is_err(),
            "rewrite bypass"
        );
        assert!(
            session.check_tool("write_file").is_err(),
            "write_file bypass"
        );
    }

    #[tokio::test]
    async fn office_pilot_readback_stage_passes_with_docx() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("deliverables")).unwrap();
        std::fs::write(dir.path().join("deliverables/report.docx"), b"pk").unwrap();

        let mut session = office_pilot_session();
        session.verified_stages.insert("prepare".into());
        session.verified_stages.insert("write".into());
        assert!(
            session
                .try_pass_stage(dir.path(), "readback_verify", None)
                .await
                .unwrap()
                .0
        );
        assert!(session.current_stage_id().is_none());
    }
}
