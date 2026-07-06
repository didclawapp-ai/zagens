//! Unified harness contract schema (Phase 2a.1).
//!
//! Skill manifests and gate manifests share the same TOML shape: `stages`, `verify`,
//! `verify_budget`, `rollback`. Predicate-native `[[verify]]` rows map to
//! [`VerifyStageSpec`](../../runtime-server) / queue gate / `HarnessVerifyLoop`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const HARNESS_CONTRACT_SCHEMA_VERSION: u32 = 1;

/// Registered predicate names (`long_horizon/predicate::*` — keep in sync).
pub mod predicates {
    pub const EXIT_CODE: &str = "exit_code";
    pub const FILE_EXISTS: &str = "file_exists";
    pub const TESTS_PASS: &str = "tests_pass";
    pub const COMMAND_OUTPUT_MATCHES: &str = "command_output_matches";
    pub const FILE_COUNT: &str = "file_count";

    pub const ALL: &[&str] = &[
        EXIT_CODE,
        FILE_EXISTS,
        TESTS_PASS,
        COMMAND_OUTPUT_MATCHES,
        FILE_COUNT,
    ];

    #[must_use]
    pub fn is_registered(name: &str) -> bool {
        ALL.iter().any(|p| *p == name)
    }
}

/// Tools always visible while a staged skill contract is active (meta / user input).
pub const STAGE_GATE_ALWAYS_ALLOWED: &[&str] = &[
    "load_skill",
    "request_user_input",
    "assert_file_count",
    "assert_output_matches",
    "assert_tests_pass",
];

/// Parsed harness contract (skill manifest = gate manifest).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessContract {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub harness: HarnessMeta,
    #[serde(default)]
    pub verify_budget: VerifyBudget,
    #[serde(default)]
    pub rollback: RollbackPolicy,
    #[serde(default)]
    pub stages: Vec<StageSpec>,
    #[serde(default)]
    pub verify: Vec<VerifyEntry>,
}

fn default_schema_version() -> u32 {
    HARNESS_CONTRACT_SCHEMA_VERSION
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessMeta {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyBudget {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_max_retries() -> u32 {
    2
}

fn default_timeout_ms() -> u64 {
    300_000
}

impl Default for VerifyBudget {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            timeout_ms: default_timeout_ms(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackPolicy {
    /// `snapshot` | `none` (future: checkpoint id)
    #[serde(default = "default_rollback_strategy")]
    pub strategy: String,
}

fn default_rollback_strategy() -> String {
    "snapshot".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageSpec {
    pub id: String,
    #[serde(default)]
    pub tools: Vec<String>,
    /// Prior stage ids whose verify must have passed before this stage unlocks.
    #[serde(default)]
    pub requires: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyEntry {
    /// Skill stage id; omit for flat gate-only manifests (queue / completion gate).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    /// Flat gate row id (Layer-2 style); optional when `stage` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub predicate: String,
    #[serde(default)]
    pub args: Value,
}

/// One row in the verify-loop (`harness_verify` telemetry).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractVerifyStage {
    pub stage: String,
    pub predicate: String,
    #[serde(default)]
    pub args: Value,
}

/// Static validation report for Gate-as-Code manifests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractValidationReport {
    pub ok: bool,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub verify_count: usize,
    #[serde(default)]
    pub stage_count: usize,
}

impl HarnessContract {
    /// Parse TOML bytes (standalone manifest file or embedded in config).
    pub fn parse_toml(raw: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(raw)
    }

    /// Parse and validate a gate / skill contract file.
    pub fn parse_and_validate(
        raw: &str,
    ) -> Result<(Self, ContractValidationReport), toml::de::Error> {
        let contract = Self::parse_toml(raw)?;
        let report = contract.validate();
        Ok((contract, report))
    }

    /// Structural + predicate registry checks (no workspace exec).
    #[must_use]
    pub fn validate(&self) -> ContractValidationReport {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if self.schema_version != HARNESS_CONTRACT_SCHEMA_VERSION {
            warnings.push(format!(
                "schema_version {} differs from current v{HARNESS_CONTRACT_SCHEMA_VERSION}",
                self.schema_version
            ));
        }

        if !self.is_active() {
            errors.push("contract has no [[stages]] and no [[verify]] rows".into());
        }

        if self.harness.id.trim().is_empty() {
            warnings.push("[harness].id is empty — set a stable id for telemetry and reuse".into());
        }

        let stage_ids: std::collections::HashSet<&str> =
            self.stages.iter().map(|s| s.id.as_str()).collect();
        if stage_ids.len() != self.stages.len() {
            errors.push("duplicate [[stages]].id values".into());
        }

        for stage in &self.stages {
            for req in &stage.requires {
                if !stage_ids.contains(req.as_str()) {
                    errors.push(format!(
                        "stage `{}` requires unknown stage `{req}`",
                        stage.id
                    ));
                }
            }
            if stage.tools.is_empty() {
                warnings.push(format!("stage `{}` exposes no tools", stage.id));
            }
        }

        for (i, entry) in self.verify.iter().enumerate() {
            let row = entry
                .id
                .as_deref()
                .or(entry.stage.as_deref())
                .unwrap_or("verify");
            if entry.predicate.trim().is_empty() {
                errors.push(format!("[[verify]] row {i} (`{row}`): missing predicate"));
                continue;
            }
            if !predicates::is_registered(&entry.predicate) {
                errors.push(format!(
                    "[[verify]] row {i} (`{row}`): unknown predicate `{}` (registered: {})",
                    entry.predicate,
                    predicates::ALL.join(", ")
                ));
            }
            if let Some(stage) = &entry.stage
                && !stage_ids.contains(stage.as_str())
            {
                errors.push(format!(
                    "[[verify]] row {i}: stage `{stage}` is not defined in [[stages]]"
                ));
            }
            if entry.stage.is_none() && entry.id.is_none() {
                warnings.push(format!(
                    "[[verify]] row {i}: flat gate row should set `id` for stable telemetry"
                ));
            }
        }

        if !self.stages.is_empty() && self.verify.iter().all(|v| v.stage.is_none()) {
            warnings.push(
                "staged contract has no stage-bound [[verify]] rows — stage gate may never advance"
                    .into(),
            );
        }

        if self.rollback.strategy != "snapshot" && self.rollback.strategy != "none" {
            warnings.push(format!(
                "[rollback].strategy `{}` is not recognized (expected snapshot|none)",
                self.rollback.strategy
            ));
        }

        ContractValidationReport {
            ok: errors.is_empty(),
            errors,
            warnings,
            verify_count: self.verify.len(),
            stage_count: self.stages.len(),
        }
    }

    /// Flat verify rows for night-queue gate (skips stage-bound skill rows).
    #[must_use]
    pub fn flat_queue_gate_rows(&self) -> Vec<ContractVerifyStage> {
        self.verify
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.stage.is_none())
            .map(|(i, entry)| ContractVerifyStage {
                stage: entry.id.clone().unwrap_or_else(|| format!("gate-{i}")),
                predicate: entry.predicate.clone(),
                args: entry.args.clone(),
            })
            .collect()
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.stages.is_empty() || !self.verify.is_empty()
    }

    /// Flat predicate verify rows (queue gate / completion gate / skill stage verify).
    #[must_use]
    pub fn verify_stages(&self) -> Vec<ContractVerifyStage> {
        self.verify
            .iter()
            .enumerate()
            .map(|(i, entry)| ContractVerifyStage {
                stage: entry
                    .stage
                    .clone()
                    .or_else(|| entry.id.clone())
                    .unwrap_or_else(|| format!("verify-{i}")),
                predicate: entry.predicate.clone(),
                args: entry.args.clone(),
            })
            .collect()
    }

    /// Verify entries bound to a skill stage id.
    #[must_use]
    pub fn verify_for_stage(&self, stage_id: &str) -> Vec<&VerifyEntry> {
        self.verify
            .iter()
            .filter(|v| v.stage.as_deref() == Some(stage_id))
            .collect()
    }

    #[must_use]
    pub fn stage_by_id(&self, id: &str) -> Option<&StageSpec> {
        self.stages.iter().find(|s| s.id == id)
    }

    /// First stage whose `requires` are satisfied but verify not yet marked passed.
    #[must_use]
    pub fn current_stage_id(&self, verified: &[String]) -> Option<String> {
        for stage in &self.stages {
            if verified.contains(&stage.id) {
                continue;
            }
            if stage.requires.iter().all(|req| verified.contains(req)) {
                return Some(stage.id.clone());
            }
        }
        None
    }

    /// Tool names the model may call at the current stage (plus always-allowed meta tools).
    #[must_use]
    pub fn allowed_tools(&self, verified: &[String]) -> Vec<String> {
        let Some(current) = self.current_stage_id(verified) else {
            return self
                .stages
                .iter()
                .flat_map(|s| s.tools.iter().cloned())
                .chain(STAGE_GATE_ALWAYS_ALLOWED.iter().map(|s| (*s).to_string()))
                .collect();
        };
        let mut out: Vec<String> = self
            .stage_by_id(&current)
            .map(|s| s.tools.clone())
            .unwrap_or_default();
        out.extend(STAGE_GATE_ALWAYS_ALLOWED.iter().map(|s| s.to_string()));
        out.sort();
        out.dedup();
        out
    }

    #[must_use]
    pub fn tool_allowed(&self, tool_name: &str, verified: &[String]) -> bool {
        if STAGE_GATE_ALWAYS_ALLOWED.contains(&tool_name) {
            return true;
        }
        let allowed = self.allowed_tools(verified);
        allowed.iter().any(|t| t == tool_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const OFFICE_FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/harness/office-write-skill-manifest.toml"
    ));

    #[test]
    fn parses_office_fixture() {
        let contract = HarnessContract::parse_toml(OFFICE_FIXTURE).expect("office fixture");
        assert_eq!(contract.harness.id, "office-write");
        assert_eq!(contract.stages.len(), 3);
        assert!(contract.verify.iter().any(|v| v.predicate == "file_count"));
    }

    #[test]
    fn stage_progression_and_tool_filter() {
        let contract = HarnessContract {
            stages: vec![
                StageSpec {
                    id: "prepare".into(),
                    tools: vec!["read_office".into()],
                    requires: vec![],
                },
                StageSpec {
                    id: "write".into(),
                    tools: vec!["write_office".into()],
                    requires: vec!["prepare".into()],
                },
            ],
            verify: vec![
                VerifyEntry {
                    stage: Some("prepare".into()),
                    id: None,
                    predicate: "file_exists".into(),
                    args: json!({"path": "x"}),
                },
                VerifyEntry {
                    stage: Some("write".into()),
                    id: None,
                    predicate: "file_exists".into(),
                    args: json!({"path": "y"}),
                },
            ],
            ..Default::default()
        };
        assert_eq!(contract.current_stage_id(&[]).as_deref(), Some("prepare"));
        assert!(contract.tool_allowed("read_office", &[]));
        assert!(!contract.tool_allowed("write_office", &[]));
        assert!(contract.tool_allowed("assert_tests_pass", &[]));

        let verified = vec!["prepare".to_string()];
        assert_eq!(
            contract.current_stage_id(&verified).as_deref(),
            Some("write")
        );
        assert!(contract.tool_allowed("write_office", &verified));
    }

    #[test]
    fn validate_rejects_unknown_predicate() {
        let contract = HarnessContract {
            verify: vec![VerifyEntry {
                stage: None,
                id: Some("x".into()),
                predicate: "not_registered".into(),
                args: json!({}),
            }],
            ..Default::default()
        };
        let report = contract.validate();
        assert!(!report.ok);
        assert!(report.errors.iter().any(|e| e.contains("not_registered")));
    }

    #[test]
    fn validate_office_fixture_ok() {
        let contract = HarnessContract::parse_toml(OFFICE_FIXTURE).expect("office fixture");
        let report = contract.validate();
        assert!(report.ok, "{:?}", report.errors);
    }

    #[test]
    fn flat_queue_gate_rows_skip_staged_verify() {
        let contract = HarnessContract::parse_toml(OFFICE_FIXTURE).expect("office fixture");
        assert!(contract.flat_queue_gate_rows().is_empty());
        let flat = HarnessContract::parse_toml(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/harness/code-edit-skill-manifest.toml"
        )))
        .expect("code-edit");
        let rows = flat.flat_queue_gate_rows();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|r| r.stage == "compile"));
    }

    #[test]
    fn verify_stages_flat_gate_rows() {
        let contract = HarnessContract {
            verify: vec![VerifyEntry {
                stage: None,
                id: Some("build".into()),
                predicate: "tests_pass".into(),
                args: json!({"toolchain": "go"}),
            }],
            ..Default::default()
        };
        let stages = contract.verify_stages();
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].stage, "build");
    }
}

impl Default for HarnessContract {
    fn default() -> Self {
        Self {
            schema_version: HARNESS_CONTRACT_SCHEMA_VERSION,
            harness: HarnessMeta::default(),
            verify_budget: VerifyBudget::default(),
            rollback: RollbackPolicy::default(),
            stages: Vec::new(),
            verify: Vec::new(),
        }
    }
}
