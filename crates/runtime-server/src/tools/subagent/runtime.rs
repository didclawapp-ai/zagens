use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::hooks::HookExecutor;
use crate::tools::spec::ToolContext;
use zagens_core::events::Event;

use super::mailbox::Mailbox;
use zagens_core::subagent::{
    CompletionReason, ParseFailureReason, StructuredFindings, StructuredVerdict,
    SubAgentAssignment, SubAgentResult, SubAgentStatus, SubAgentType,
};

use super::constants::STEP_API_TIMEOUT;
use super::factory::SharedSubAgentManager;
use super::types::DEFAULT_MAX_SPAWN_DEPTH;
use super::types::SubAgentInput;

/// Terminal-state notification emitted to the engine's parent turn loop
/// when one of its direct children finishes (issue #756). Carries the
/// already-rendered `<deepseek:subagent.done>` sentinel that the model
/// expects in the transcript per `prompts/base.md`.
#[derive(Debug, Clone)]
pub struct SubAgentCompletion {
    /// The completing child's agent id. Held for routing/logging — the
    /// engine's turn loop does not currently key on it (it just injects
    /// the payload), but downstream tooling and tests need the field.
    #[allow(dead_code)]
    pub agent_id: String,
    /// Human summary on line 1, sentinel on line 2. Same payload shape as
    /// `Event::AgentComplete::result`.
    pub payload: String,
}

/// Runtime configuration for spawning sub-agents.
///
/// Carries everything a child needs to (a) build its own tool registry —
/// including the manager so grandchildren can spawn — and (b) cooperate
/// with the rest of the spawn tree on cancellation and depth cap.
#[derive(Clone)]
pub struct SubAgentRuntime {
    pub client: Arc<dyn crate::llm_client::LlmClient>,
    pub model: String,
    pub auto_model: bool,
    pub reasoning_effort: Option<String>,
    pub reasoning_effort_auto: bool,
    pub role_models: HashMap<String, String>,
    pub context: ToolContext,
    pub allow_shell: bool,
    pub event_tx: Option<mpsc::Sender<Event>>,
    /// Manager handle so children can recurse via `agent_spawn`. All agents
    /// at every depth share the same manager.
    pub manager: SharedSubAgentManager,
    /// Depth in the spawn tree. 0 = top-level user turn; 1 = direct child;
    /// etc. Children clone the parent runtime and increment this on spawn.
    pub spawn_depth: u32,
    /// Hard cap on recursion depth. A child whose `spawn_depth + 1` would
    /// exceed this is rejected at the spawn entry. Use `>` (strictly
    /// greater than) so equality is allowed — matches codex's pattern.
    pub max_spawn_depth: u32,
    /// Cooperative cancellation token. Children derive a child_token() from
    /// the parent so cancelling the root cascades down.
    pub cancel_token: CancellationToken,
    /// Structured progress / lifecycle stream. Cloned across children so the
    /// whole spawn tree publishes into one ordered, fan-out-able mailbox.
    /// `None` only when no consumer is wired (legacy entry points / tests).
    pub mailbox: Option<Mailbox>,
    /// Wakeup channel for the engine's parent turn loop (issue #756). Only
    /// the engine's direct children fire on this — propagated to descendants
    /// via clone but gated to `spawn_depth == 1` at the send site so the
    /// parent isn't flooded with grandchild completions it didn't directly
    /// orchestrate. `None` when no consumer is wired (tests / legacy paths).
    pub parent_completion_tx: Option<mpsc::UnboundedSender<SubAgentCompletion>>,
    /// Per-step LLM API call timeout. Each `create_message` request must
    /// complete within this window or the step is treated as timed out.
    /// Defaults to [`STEP_API_TIMEOUT`] (120 s). Increase for review/audit
    /// workloads where a single step may read many files.
    pub step_timeout: Duration,
    /// Lifecycle hook executor propagated from the parent engine (optional).
    pub hook_executor: Option<Arc<HookExecutor>>,
}

impl SubAgentRuntime {
    /// Create a top-level runtime configuration for sub-agent execution.
    /// Use this from the engine when constructing the runtime that the
    /// parent's tool registry passes through. Children should derive their
    /// runtime via `Self::child_runtime` instead.
    #[must_use]
    pub fn new(
        client: Arc<dyn crate::llm_client::LlmClient>,
        model: String,
        context: ToolContext,
        allow_shell: bool,
        event_tx: Option<mpsc::Sender<Event>>,
        manager: SharedSubAgentManager,
    ) -> Self {
        Self {
            client,
            model,
            auto_model: false,
            reasoning_effort: None,
            reasoning_effort_auto: false,
            role_models: HashMap::new(),
            context,
            allow_shell,
            event_tx,
            manager,
            spawn_depth: 0,
            max_spawn_depth: DEFAULT_MAX_SPAWN_DEPTH,
            cancel_token: CancellationToken::new(),
            mailbox: None,
            parent_completion_tx: None,
            step_timeout: STEP_API_TIMEOUT,
            hook_executor: None,
        }
    }

    /// Attach the wakeup channel so the engine's parent turn loop can resume
    /// when this runtime's direct children finish (issue #756). The channel
    /// is propagated to descendants via clone, but only `spawn_depth == 1`
    /// agents fire on it — see `run_subagent_task`.
    #[must_use]
    pub fn with_parent_completion_tx(
        mut self,
        tx: mpsc::UnboundedSender<SubAgentCompletion>,
    ) -> Self {
        self.parent_completion_tx = Some(tx);
        self
    }

    /// Attach a `Mailbox` so this runtime (and every descendant — children
    /// clone it) publishes structured `MailboxMessage` envelopes alongside
    /// the legacy `Event` stream. Pair with [`Self::with_cancel_token`] when
    /// you want close-as-cancel to propagate the same way.
    #[must_use]
    #[allow(dead_code)] // wired by #128 (in-transcript cards) when it lands.
    pub fn with_mailbox(mut self, mailbox: Mailbox) -> Self {
        self.mailbox = Some(mailbox);
        self
    }

    /// Replace the cancellation token (e.g. when the engine constructs the
    /// runtime alongside a mailbox bound to the same token).
    #[must_use]
    #[allow(dead_code)] // wired by #128 alongside `with_mailbox`.
    pub fn with_cancel_token(mut self, token: CancellationToken) -> Self {
        self.cancel_token = token;
        self
    }

    /// Override the maximum spawn depth (default `DEFAULT_MAX_SPAWN_DEPTH`).
    /// Used by config wiring (`[runtime] max_spawn_depth = N`) and tests.
    #[must_use]
    #[allow(dead_code)]
    pub fn with_max_spawn_depth(mut self, max: u32) -> Self {
        self.max_spawn_depth = max;
        self
    }

    /// Override the per-step API timeout (default [`STEP_API_TIMEOUT`] = 120 s).
    /// Increase for review/audit workloads where each child step may read
    /// many files before returning.
    #[must_use]
    pub fn with_step_timeout(mut self, timeout: Duration) -> Self {
        self.step_timeout = timeout;
        self
    }

    /// Attach the parent engine's hook executor for sub-agent lifecycle hooks.
    #[must_use]
    pub fn with_hook_executor(mut self, executor: Arc<HookExecutor>) -> Self {
        self.hook_executor = Some(executor);
        self
    }

    /// Attach raw role/type model overrides. Values are intentionally
    /// validated at spawn time so bad config fails before a partial spawn.
    #[must_use]
    pub fn with_role_models(mut self, role_models: HashMap<String, String>) -> Self {
        self.role_models = role_models;
        self
    }

    /// Preserve whether the parent session is using per-turn model routing.
    #[must_use]
    pub fn with_auto_model(mut self, auto_model: bool) -> Self {
        self.auto_model = auto_model;
        self
    }

    /// Preserve the parent's thinking configuration. `reasoning_effort_auto`
    /// stays true even when the parent turn itself was sent with a concrete
    /// flash-router recommendation, so children can resolve their own tier.
    #[must_use]
    pub fn with_reasoning_effort(
        mut self,
        reasoning_effort: Option<String>,
        reasoning_effort_auto: bool,
    ) -> Self {
        self.reasoning_effort = reasoning_effort;
        self.reasoning_effort_auto = reasoning_effort_auto;
        self
    }

    /// Return a child runtime that is deliberately detached from the parent
    /// turn cancellation token. Background sub-agents should keep running when
    /// the parent turn is cancelled; explicit agent cancellation still
    /// aborts their task handles through the manager.
    #[must_use]
    pub fn background_runtime(&self) -> Self {
        let mut runtime = self.child_runtime();
        let token = CancellationToken::new();
        runtime.cancel_token = token.clone();
        runtime.context.cancel_token = Some(token);
        runtime
    }

    /// Build a child runtime cloning this one, incrementing `spawn_depth`,
    /// deriving a child cancellation token, and forcing `auto_approve` on
    /// the child's `ToolContext`. Used at spawn entry to construct the
    /// runtime the new sub-agent will see.
    ///
    /// The `auto_approve` override is deliberate: spawning IS the approval.
    /// Per-tool prompts inside a child would break delegation, so children
    /// inherit a YOLO-equivalent context regardless of the parent's mode.
    /// The workspace boundary + sandbox profile still apply.
    #[must_use]
    pub fn child_runtime(&self) -> Self {
        let mut child_context = self.context.clone();
        child_context.auto_approve = true;
        Self {
            client: self.client.clone(),
            model: self.model.clone(),
            auto_model: self.auto_model,
            reasoning_effort: self.reasoning_effort.clone(),
            reasoning_effort_auto: self.reasoning_effort_auto,
            role_models: self.role_models.clone(),
            context: child_context,
            allow_shell: self.allow_shell,
            event_tx: self.event_tx.clone(),
            manager: self.manager.clone(),
            spawn_depth: self.spawn_depth + 1,
            max_spawn_depth: self.max_spawn_depth,
            cancel_token: self.cancel_token.child_token(),
            mailbox: self.mailbox.clone(),
            parent_completion_tx: self.parent_completion_tx.clone(),
            step_timeout: self.step_timeout,
            hook_executor: self.hook_executor.clone(),
        }
    }

    /// Whether the next spawn would exceed the depth cap.
    #[must_use]
    pub fn would_exceed_depth(&self) -> bool {
        self.spawn_depth + 1 > self.max_spawn_depth
    }
}

/// A running sub-agent instance.
pub struct SubAgent {
    pub id: String,
    pub agent_type: SubAgentType,
    pub prompt: String,
    pub assignment: SubAgentAssignment,
    pub model: String,
    pub nickname: Option<String>,
    pub status: SubAgentStatus,
    pub result: Option<String>,
    pub structured_verdict: Option<StructuredVerdict>,
    pub structured_findings: Option<StructuredFindings>,
    pub completion_reason: Option<CompletionReason>,
    pub structured_findings_parse_failure: Option<ParseFailureReason>,
    pub blackboard_task_id: Option<String>,
    pub scratchpad_run_id: Option<String>,
    /// Parent runtime thread at spawn time (`RuntimeToolHostWire::active_thread_id`).
    pub parent_thread_id: Option<String>,
    pub step_timeout: Duration,
    pub max_steps: u32,
    pub steps_taken: u32,
    pub started_at: Instant,
    pub last_progress_at: Instant,
    pub progress_status: Option<String>,
    /// `None` = full registry inheritance (v0.6.6 default).
    /// `Some(list)` = explicit narrow allowlist (Custom agents, legacy).
    pub allowed_tools: Option<Vec<String>>,
    /// Stable id of the manager that spawned this agent (#405). Compared
    /// against the manager's `current_session_boot_id` to classify the
    /// agent as in-session vs prior-session at list time.
    pub session_boot_id: String,
    pub(crate) input_tx: Option<mpsc::UnboundedSender<SubAgentInput>>,
    pub(crate) task_handle: Option<JoinHandle<()>>,
}

impl SubAgent {
    /// Create a new sub-agent.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        agent_type: SubAgentType,
        prompt: String,
        assignment: SubAgentAssignment,
        model: String,
        nickname: Option<String>,
        allowed_tools: Option<Vec<String>>,
        step_timeout: Duration,
        max_steps: u32,
        input_tx: mpsc::UnboundedSender<SubAgentInput>,
        session_boot_id: String,
    ) -> Self {
        let id = format!("agent_{}", &Uuid::new_v4().to_string()[..8]);
        let started_at = Instant::now();

        Self {
            id,
            agent_type,
            prompt,
            assignment,
            model,
            nickname,
            status: SubAgentStatus::Running,
            result: None,
            structured_verdict: None,
            structured_findings: None,
            completion_reason: None,
            structured_findings_parse_failure: None,
            blackboard_task_id: None,
            scratchpad_run_id: None,
            parent_thread_id: None,
            step_timeout,
            max_steps,
            steps_taken: 0,
            started_at,
            last_progress_at: started_at,
            progress_status: None,
            allowed_tools,
            session_boot_id,
            input_tx: Some(input_tx),
            task_handle: None,
        }
    }

    /// Get a snapshot of the current state.
    #[must_use]
    pub fn snapshot(&self) -> SubAgentResult {
        let idle = self.last_progress_at.elapsed();
        let idle_ms = u64::try_from(idle.as_millis()).unwrap_or(u64::MAX);
        let stuck_suspected =
            super::constants::compute_stuck_suspected(&self.status, self.step_timeout, idle);
        SubAgentResult {
            agent_id: self.id.clone(),
            agent_type: self.agent_type.clone(),
            assignment: self.assignment.clone(),
            model: self.model.clone(),
            nickname: self.nickname.clone(),
            status: self.status.clone(),
            result: self.result.clone(),
            steps_taken: self.steps_taken,
            duration_ms: u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
            // Snapshots from the agent itself don't know the manager's
            // current boot id, so default to false. The manager fills
            // this in when it produces a snapshot via its own
            // `snapshot_for_listing` helper (#405).
            from_prior_session: false,
            structured_verdict: self.structured_verdict.clone(),
            structured_findings: self.structured_findings.clone(),
            completion_reason: self.completion_reason.clone(),
            max_steps: self.max_steps,
            step_timeout_ms: u64::try_from(self.step_timeout.as_millis()).unwrap_or(u64::MAX),
            structured_findings_parse_failure: self.structured_findings_parse_failure.clone(),
            scratchpad_run_id: self.scratchpad_run_id.clone(),
            parent_thread_id: self.parent_thread_id.clone(),
            progress_status: self.progress_status.clone(),
            stuck_suspected,
            idle_ms,
        }
    }
}
