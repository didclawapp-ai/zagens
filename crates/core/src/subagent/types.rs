//! Sub-agent snapshot types shared with engine events (P2 PR4 → `zagens-core`).

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

/// Why a sub-agent reached its terminal state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum CompletionReason {
    NaturalBreak,
    StepLimitReached,
    Cancelled,
    StepApiTimeout,
    Panic(String),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Why structured `<!-- audit-findings -->` parsing failed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ParseFailureReason {
    NoMarker,
    Truncated,
    InvalidJson(String),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_reason: Option<CompletionReason>,
    /// Maximum steps this agent may take (spawn-time cap).
    #[serde(default = "default_subagent_max_steps")]
    pub max_steps: u32,
    /// Per-step LLM API timeout in milliseconds (spawn-time value).
    #[serde(default = "default_subagent_step_timeout_ms")]
    pub step_timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_findings_parse_failure: Option<ParseFailureReason>,
    /// Scratchpad run this agent was spawned against (audit isolation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratchpad_run_id: Option<String>,
    /// Parent runtime thread that spawned this sub-agent (UI / `agent_list` isolation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    /// Latest execution progress line (also emitted as `agent.progress`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_status: Option<String>,
    /// Running agent with no progress longer than `step_timeout_ms` + buffer.
    #[serde(default, skip_serializing_if = "is_false")]
    pub stuck_suspected: bool,
    /// Milliseconds since the last progress heartbeat.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub idle_ms: u64,
}

fn default_subagent_max_steps() -> u32 {
    100
}

fn default_subagent_step_timeout_ms() -> u64 {
    600_000
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero_u64(n: &u64) -> bool {
    *n == 0
}

impl SubAgentAssignment {
    #[must_use]
    pub fn new(objective: String, role: Option<String>) -> Self {
        Self { objective, role }
    }
}

impl SubAgentType {
    #[must_use]
    #[allow(clippy::should_implement_trait)]
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
