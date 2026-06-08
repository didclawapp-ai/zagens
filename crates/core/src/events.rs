//! Events emitted by the engine to the UI (P2 PR4 → `zagens-core`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::chat::{Message, SystemPrompt};
use crate::coherence::CoherenceState;
use crate::cycle::CycleBriefing;
use crate::error_taxonomy::ErrorEnvelope;
use crate::models::Usage;
use crate::subagent::{MailboxMessage, SubAgentResult};
use crate::turn::TurnOutcomeStatus;
use crate::user_input::UserInputRequest;
use zagens_tools::{ToolError, ToolResult};

/// Events emitted by the engine to update the UI.
#[derive(Debug, Clone)]
pub enum Event {
    MessageStarted {
        #[allow(dead_code)]
        index: usize,
    },
    MessageDelta {
        #[allow(dead_code)]
        index: usize,
        content: String,
    },
    MessageComplete {
        #[allow(dead_code)]
        index: usize,
    },
    ThinkingStarted {
        #[allow(dead_code)]
        index: usize,
    },
    ThinkingDelta {
        #[allow(dead_code)]
        index: usize,
        content: String,
    },
    ThinkingComplete {
        #[allow(dead_code)]
        index: usize,
    },
    ToolCallStarted {
        id: String,
        name: String,
        input: Value,
    },
    ToolCallProgress {
        id: String,
        output: String,
    },
    ToolCallComplete {
        id: String,
        name: String,
        result: Result<ToolResult, ToolError>,
    },
    TurnStarted {
        turn_id: String,
    },
    TurnComplete {
        usage: Usage,
        last_request_input_tokens: Option<u32>,
        status: TurnOutcomeStatus,
        error: Option<String>,
        step_count: u32,
        tool_names: Vec<String>,
        end_reason: Option<String>,
    },
    CompactionStarted {
        id: String,
        auto: bool,
        message: String,
    },
    CompactionCompleted {
        id: String,
        auto: bool,
        message: String,
        #[allow(dead_code)]
        messages_before: Option<usize>,
        #[allow(dead_code)]
        messages_after: Option<usize>,
    },
    CompactionFailed {
        id: String,
        auto: bool,
        message: String,
    },
    CycleAdvanced {
        from: u32,
        to: u32,
        briefing: CycleBriefing,
    },
    #[allow(dead_code)]
    CapacityDecision {
        session_id: String,
        turn_id: String,
        h_hat: f64,
        c_hat: f64,
        slack: f64,
        min_slack: f64,
        violation_ratio: f64,
        p_fail: f64,
        risk_band: String,
        action: String,
        cooldown_blocked: bool,
        reason: String,
    },
    #[allow(dead_code)]
    CapacityIntervention {
        session_id: String,
        turn_id: String,
        action: String,
        before_prompt_tokens: usize,
        after_prompt_tokens: usize,
        compaction_size_reduction: usize,
        replay_outcome: Option<String>,
        replan_performed: bool,
    },
    #[allow(dead_code)]
    CapacityMemoryPersistFailed {
        session_id: String,
        turn_id: String,
        action: String,
        error: String,
    },
    CoherenceState {
        state: CoherenceState,
        label: String,
        description: String,
        reason: String,
    },
    AgentSpawned {
        id: String,
        prompt: String,
    },
    AgentProgress {
        id: String,
        status: String,
    },
    AgentComplete {
        id: String,
        result: String,
    },
    AgentList {
        agents: Vec<SubAgentResult>,
    },
    SubAgentMailbox {
        seq: u64,
        message: MailboxMessage,
    },
    Error {
        envelope: ErrorEnvelope,
        #[allow(dead_code)]
        recoverable: bool,
    },
    Status {
        message: String,
    },
    PauseEvents,
    ResumeEvents,
    ApprovalRequired {
        id: String,
        tool_name: String,
        description: String,
        approval_key: String,
    },
    UserInputRequired {
        id: String,
        request: UserInputRequest,
    },
    SessionUpdated {
        messages: Vec<Message>,
        system_prompt: Option<SystemPrompt>,
        model: String,
        workspace: PathBuf,
    },
    /// CRAFT: structured verdict from a completing sub-agent (B-L1).
    CraftVerdict {
        agent_id: String,
        agent_type: String,
        task_id: Option<String>,
        verdict: String,
        summary: Option<String>,
        items: Value,
    },
    /// CRAFT: blackboard partition updated under `.deepseek/blackboards/` (B-L1).
    CraftBoardUpdated {
        task_id: String,
        partition: String,
        agent_id: String,
    },
    #[allow(dead_code)]
    ElevationRequired {
        tool_id: String,
        tool_name: String,
        command: Option<String>,
        denial_reason: String,
        blocked_network: bool,
        blocked_write: bool,
    },
}

/// Structured turn outcome summary (A2.1 — step/tools/end reason for logs and runtime events).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnSummary {
    pub step_count: u32,
    pub tool_names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_reason: Option<String>,
}

impl TurnSummary {
    #[must_use]
    pub fn new(step_count: u32, tool_names: Vec<String>, end_reason: Option<String>) -> Self {
        Self {
            step_count,
            tool_names,
            end_reason,
        }
    }

    #[must_use]
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// Structured log line aligned with runtime `turn_summary` / `turn.completed`.
    pub fn log_turn_complete(
        &self,
        turn_id: &str,
        status: TurnOutcomeStatus,
        thread_id: Option<&str>,
    ) {
        match thread_id {
            Some(thread_id) => tracing::info!(
                thread_id = %thread_id,
                turn_id = %turn_id,
                step_count = self.step_count,
                tool_count = self.tool_names.len(),
                tools = ?self.tool_names,
                end_reason = self.end_reason.as_deref(),
                ?status,
                "turn complete"
            ),
            None => tracing::info!(
                turn_id = %turn_id,
                step_count = self.step_count,
                tool_count = self.tool_names.len(),
                tools = ?self.tool_names,
                end_reason = self.end_reason.as_deref(),
                ?status,
                "turn complete"
            ),
        }
    }
}

impl Event {
    #[must_use]
    pub fn error(envelope: ErrorEnvelope) -> Self {
        let recoverable = envelope.recoverable;
        Event::Error {
            envelope,
            recoverable,
        }
    }

    #[must_use]
    pub fn status(message: impl Into<String>) -> Self {
        Event::Status {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn turn_summary_serializes_for_runtime_payload() {
        let summary = TurnSummary::new(
            2,
            vec!["read_file".to_string()],
            Some("completed".to_string()),
        );
        assert_eq!(
            summary.to_value(),
            json!({
                "step_count": 2,
                "tool_names": ["read_file"],
                "end_reason": "completed",
            })
        );
    }

    #[test]
    fn turn_summary_omits_null_end_reason() {
        let summary = TurnSummary::new(0, vec![], None);
        let value = summary.to_value();
        assert_eq!(value.get("step_count").and_then(|v| v.as_u64()), Some(0));
        assert!(value.get("end_reason").is_none());
    }
}
