//! Sub-agent snapshot types shared with engine events (P2 PR4 → `deepseek-core`).

use serde::{Deserialize, Serialize};

/// Assignment metadata for sub-agent orchestration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubAgentAssignment {
    pub objective: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Sub-agent execution types with specialized behavior and tool access.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubAgentType {
    #[default]
    General,
    Explore,
    Plan,
    Review,
    Implementer,
    Verifier,
    Custom,
    Auditor,
}

/// Status of a sub-agent execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SubAgentStatus {
    Running,
    Completed,
    Interrupted(String),
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VerdictLevel {
    #[serde(rename = "PASS")]
    Pass,
    #[serde(rename = "BLOCKER")]
    Blocker,
    #[serde(rename = "MAJOR")]
    Major,
    #[serde(rename = "FAIL")]
    Fail,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerdictItem {
    pub severity: String,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredVerdict {
    pub verdict: VerdictLevel,
    pub items: Vec<VerdictItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// One machine-readable audit finding from an Explore/Review sub-agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditFindingItem {
    #[serde(default = "default_finding_kind")]
    pub kind: String,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_end: Option<u32>,
    pub claim: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

fn default_finding_kind() -> String {
    "finding".to_string()
}

/// Structured audit output (`<!-- audit-findings -->`) for scratchpad import.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredFindings {
    pub area_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_path: Option<String>,
    pub items: Vec<AuditFindingItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Snapshot of sub-agent state for tool results and `Event::AgentList`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentResult {
    pub agent_id: String,
    pub agent_type: SubAgentType,
    pub assignment: SubAgentAssignment,
    #[serde(default)]
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    pub status: SubAgentStatus,
    pub result: Option<String>,
    pub steps_taken: u32,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub from_prior_session: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_verdict: Option<StructuredVerdict>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_findings: Option<StructuredFindings>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl SubAgentAssignment {
    #[must_use]
    pub fn new(objective: String, role: Option<String>) -> Self {
        Self { objective, role }
    }
}

impl SubAgentType {
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "general" | "general-purpose" | "general_purpose" | "worker" | "default" => {
                Some(Self::General)
            }
            "explore" | "exploration" | "explorer" => Some(Self::Explore),
            "plan" | "planning" | "awaiter" => Some(Self::Plan),
            "review" | "code-review" | "code_review" | "reviewer" => Some(Self::Review),
            "implementer" | "implement" | "implementation" | "builder" => Some(Self::Implementer),
            "verifier" | "verify" | "verification" | "validator" | "tester" => Some(Self::Verifier),
            "custom" => Some(Self::Custom),
            "auditor" | "audit" | "fact-checker" | "fact_checker" => Some(Self::Auditor),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Explore => "explore",
            Self::Plan => "plan",
            Self::Review => "review",
            Self::Implementer => "implementer",
            Self::Verifier => "verifier",
            Self::Custom => "custom",
            Self::Auditor => "auditor",
        }
    }
}
