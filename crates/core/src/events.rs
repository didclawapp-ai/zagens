//! Events emitted by the engine to the UI (P2 PR4 → `deepseek-core`).

use std::path::PathBuf;

use serde_json::Value;

use crate::chat::{Message, SystemPrompt};
use crate::coherence::CoherenceState;
use crate::cycle::CycleBriefing;
use crate::error_taxonomy::ErrorEnvelope;
use crate::models::Usage;
use crate::subagent::{MailboxMessage, SubAgentResult};
use crate::turn::TurnOutcomeStatus;
use crate::user_input::UserInputRequest;
use deepseek_tools::{ToolError, ToolResult};

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
