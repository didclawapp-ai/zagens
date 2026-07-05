//! Unified harness contract schema (Phase 2a.1).
//!
//! Skill manifests and gate manifests share the same TOML shape: `stages`, `verify`,
//! `verify_budget`, `rollback`. Predicate-native `[[verify]]` rows map to
//! [`VerifyStageSpec`](../../runtime-server) / queue gate / `HarnessVerifyLoop`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const HARNESS_CONTRACT_SCHEMA_VERSION: u32 = 1;

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

impl HarnessContract {
    /// Parse TOML bytes (standalone manifest file or embedded in config).
    pub fn parse_toml(raw: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(raw)
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
