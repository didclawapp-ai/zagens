use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use tokio::sync::mpsc;
use uuid::Uuid;

use deepseek_core::events::Event;
use crate::utils::spawn_supervised;

use deepseek_core::subagent::{
    CompletionReason, SubAgentAssignment, SubAgentResult, SubAgentStatus,
    SubAgentType,
};

use super::constants::*;
use super::registry::build_allowed_tools;
use super::resident::release_resident_leases_for;
use super::executor::{run_subagent_task, SubAgentTask};
use super::parse::normalize_role_alias;
use super::types::SubAgentInput;
use super::factory::SharedSubAgentManager;
use super::runtime::{SubAgent, SubAgentRuntime};
use super::structured_fallback::enrich_subagent_result;
use super::factory::{epoch_millis_now, instant_from_duration, write_json_atomic};
use super::types::{PersistedSubAgent, PersistedSubAgentState, SubAgentSpawnOptions};

pub struct SubAgentManager {
    pub(crate) agents: HashMap<String, SubAgent>,
    #[allow(dead_code)] // Stored for future workspace-scoped operations
    workspace: PathBuf,
    state_path: Option<PathBuf>,
    max_steps: u32,
    max_agents: usize,
    /// Stable id assigned at manager construction (#405). Stamped on
    /// every agent the manager spawns; agents loaded from the
    /// persisted state file carry whatever id the prior session
    /// stamped (or empty for pre-#405 records). The manager classifies
    /// agents whose `session_boot_id` doesn't match this value as
    /// "from prior session" so `agent_list` can hide them by default.
    current_session_boot_id: String,
}

impl SubAgentManager {
    /// Create a new manager for sub-agents.
    #[must_use]
    pub fn new(workspace: PathBuf, max_agents: usize) -> Self {
        Self {
            agents: HashMap::new(),
            workspace,
            state_path: None,
            max_steps: DEFAULT_MAX_STEPS,
            max_agents,
            // Fresh boot id per manager. Used by #405 to classify
            // re-loaded persisted agents as "prior session".
            current_session_boot_id: format!("boot_{}", &Uuid::new_v4().to_string()[..12]),
        }
    }

    /// Return the boot id this manager stamps on agents it spawns.
    /// Exposed for tests; internal callers use the field directly.
    #[cfg(test)]
    pub fn session_boot_id(&self) -> &str {
        &self.current_session_boot_id
    }

    /// Classify an agent by its `session_boot_id`: `true` when the
    /// agent was either (a) loaded from disk with no id, or (b) carries
    /// a different id than the manager's current boot. Filters
    /// `agent_list` output by default (#405).
    fn is_from_prior_session(&self, agent: &SubAgent) -> bool {
        agent.session_boot_id.is_empty() || agent.session_boot_id != self.current_session_boot_id
    }

    #[must_use]
    pub(crate) fn with_state_path(mut self, path: PathBuf) -> Self {
        self.state_path = Some(path);
        self
    }

    pub(crate) fn persist_state(&self) -> Result<()> {
        let Some(path) = self.state_path.as_ref() else {
            return Ok(());
        };
        let now_ms = epoch_millis_now();
        let mut agents = Vec::with_capacity(self.agents.len());
        for agent in self.agents.values() {
            agents.push(PersistedSubAgent {
                id: agent.id.clone(),
                agent_type: agent.agent_type.clone(),
                prompt: agent.prompt.clone(),
                assignment: agent.assignment.clone(),
                model: agent.model.clone(),
                nickname: agent.nickname.clone(),
                status: agent.status.clone(),
                result: agent.result.clone(),
                steps_taken: agent.steps_taken,
                duration_ms: u64::try_from(agent.started_at.elapsed().as_millis())
                    .unwrap_or(u64::MAX),
                // Backward-compat: Vec on disk. None → empty vec; Some(list) → list.
                // Reload converts empty vec back to None (full inheritance).
                allowed_tools: agent.allowed_tools.clone().unwrap_or_default(),
                updated_at_ms: now_ms,
                progress_status: agent.progress_status.clone(),
                max_steps: agent.max_steps,
                step_timeout_ms: u64::try_from(agent.step_timeout.as_millis()).unwrap_or(u64::MAX),
                session_boot_id: agent.session_boot_id.clone(),
                completion_reason: agent.completion_reason.clone(),
                blackboard_task_id: agent.blackboard_task_id.clone(),
                scratchpad_run_id: agent.scratchpad_run_id.clone(),
            });
        }
        agents.sort_by(|a, b| a.id.cmp(&b.id));

        let payload = PersistedSubAgentState {
            schema_version: SUBAGENT_STATE_SCHEMA_VERSION,
            agents,
        };
        write_json_atomic(path, &payload)
    }

    fn persist_state_best_effort(&self) {
        if let Err(err) = self.persist_state() {
            eprintln!("Failed to persist sub-agent state: {err}");
        }
    }

    pub(crate) fn load_state(&mut self) -> Result<()> {
        let Some(path) = self.state_path.as_ref() else {
            return Ok(());
        };
        if !path.exists() {
            return Ok(());
        }

        let raw = fs::read_to_string(path)?;
        let state = serde_json::from_str::<PersistedSubAgentState>(&raw)?;
        if state.schema_version != SUBAGENT_STATE_SCHEMA_VERSION {
            return Err(anyhow!(
                "Unsupported sub-agent state schema {}",
                state.schema_version
            ));
        }

        self.agents.clear();
        for persisted in state.agents {
            let mut status = persisted.status;
            if matches!(status, SubAgentStatus::Running) {
                status = SubAgentStatus::Interrupted(SUBAGENT_RESTART_REASON.to_string());
            }

            let started_at = instant_from_duration(Duration::from_millis(persisted.duration_ms));
            // Empty vec on disk → None (full inheritance, v0.6.6 default).
            // Non-empty vec → Some(list) (preserves narrow scope from older sessions).
            let allowed_tools = if persisted.allowed_tools.is_empty() {
                None
            } else {
                Some(persisted.allowed_tools)
            };
            let agent = SubAgent {
                id: persisted.id.clone(),
                agent_type: persisted.agent_type,
                prompt: persisted.prompt,
                assignment: persisted.assignment,
                model: if persisted.model.is_empty() {
                    "unknown".to_string()
                } else {
                    persisted.model
                },
                nickname: persisted.nickname,
                status,
                result: persisted.result,
                structured_verdict: None,
                structured_findings: None,
                structured_findings_parse_failure: None,
                completion_reason: persisted.completion_reason,
                step_timeout: if persisted.step_timeout_ms > 0 {
                    Duration::from_millis(persisted.step_timeout_ms)
                } else {
                    STEP_API_TIMEOUT
                },
                max_steps: if persisted.max_steps > 0 {
                    persisted.max_steps
                } else {
                    DEFAULT_MAX_STEPS
                },
                steps_taken: persisted.steps_taken,
                started_at,
                last_progress_at: started_at,
                progress_status: persisted.progress_status.clone(),
                allowed_tools,
                session_boot_id: persisted.session_boot_id,
                blackboard_task_id: persisted.blackboard_task_id,
                scratchpad_run_id: persisted.scratchpad_run_id,
                input_tx: None,
                task_handle: None,
            };
            self.agents.insert(persisted.id, agent);
        }

        Ok(())
    }

    /// Count running agents.
    pub fn running_count(&mut self) -> usize {
        self.ensure_consistency();
        self.agents
            .values()
            .filter(|agent| {
                // Exclude non-running statuses
                if agent.status != SubAgentStatus::Running {
                    return false;
                }
                // Exclude persisted agents with no task_handle (they're not actually running)
                let Some(handle) = agent.task_handle.as_ref() else {
                    return false;
                };
                // Exclude agents whose task has finished (status will be updated to Completed shortly)
                !handle.is_finished()
            })
            .count()
    }

    /// Spawn a new background sub-agent.
    pub fn spawn_background(
        &mut self,
        manager_handle: SharedSubAgentManager,
        runtime: SubAgentRuntime,
        agent_type: SubAgentType,
        prompt: String,
        allowed_tools: Option<Vec<String>>,
    ) -> Result<SubAgentResult> {
        self.spawn_background_with_assignment(
            manager_handle,
            runtime,
            agent_type,
            prompt.clone(),
            SubAgentAssignment::new(prompt, None),
            allowed_tools,
        )
    }

    /// Spawn a new background sub-agent with explicit assignment metadata.
    pub fn spawn_background_with_assignment(
        &mut self,
        manager_handle: SharedSubAgentManager,
        runtime: SubAgentRuntime,
        agent_type: SubAgentType,
        prompt: String,
        assignment: SubAgentAssignment,
        allowed_tools: Option<Vec<String>>,
    ) -> Result<SubAgentResult> {
        self.spawn_background_with_assignment_options(
            manager_handle,
            runtime,
            agent_type,
            prompt,
            assignment,
            allowed_tools,
            SubAgentSpawnOptions::default(),
        )
    }

    /// Spawn a new background sub-agent with explicit assignment and display
    /// metadata.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn_background_with_assignment_options(
        &mut self,
        manager_handle: SharedSubAgentManager,
        mut runtime: SubAgentRuntime,
        agent_type: SubAgentType,
        prompt: String,
        assignment: SubAgentAssignment,
        allowed_tools: Option<Vec<String>>,
        options: SubAgentSpawnOptions,
    ) -> Result<SubAgentResult> {
        self.cleanup(COMPLETED_AGENT_RETENTION);

        let running = self.running_count();
        if running >= self.max_agents {
            return Err(anyhow!(
                "Sub-agent limit reached (max {}, running {}). Cancel, close, or wait for an existing agent to finish. Consider issuing multiple tool calls in one turn (the dispatcher runs them in parallel) for parallel one-shot work.",
                self.max_agents,
                running
            ));
        }

        if let Some(model) = options.model.as_deref() {
            runtime.model = model.to_string();
        }
        let effective_model = runtime.model.clone();
        let nickname = options
            .nickname
            .or_else(|| Some(whale_nickname_for_index(self.agents.len())));
        let tools = build_allowed_tools(&agent_type, allowed_tools, runtime.allow_shell)?;
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let max_steps = self.max_steps;
        let mut agent = SubAgent::new(
            agent_type.clone(),
            prompt.clone(),
            assignment.clone(),
            effective_model,
            nickname,
            tools.clone(),
            runtime.step_timeout,
            max_steps,
            input_tx,
            self.current_session_boot_id.clone(),
        );
        let agent_id = agent.id.clone();
        let started_at = agent.started_at;

        if let Some(event_tx) = runtime.event_tx.clone() {
            let _ = event_tx.try_send(Event::AgentSpawned {
                id: agent_id.clone(),
                prompt: prompt.clone(),
            });
        }

        let task = SubAgentTask {
            manager_handle,
            runtime,
            agent_id: agent_id.clone(),
            agent_type,
            prompt,
            assignment,
            allowed_tools: tools,
            started_at,
            max_steps,
            input_rx,
            task_id: options.task_id.clone(),
        };
        let handle = spawn_supervised(
            "subagent-task",
            std::panic::Location::caller(),
            run_subagent_task(task),
        );
        agent.task_handle = Some(handle);
        agent.blackboard_task_id = options.task_id.clone();
        agent.scratchpad_run_id = options.scratchpad_run_id.clone();
        self.agents.insert(agent_id.clone(), agent);
        self.persist_state_best_effort();

        Ok(self
            .agents
            .get(&agent_id)
            .expect("agent should exist after spawn")
            .snapshot())
    }

    /// Get the current snapshot for an agent.
    pub fn get_result(&mut self, agent_id: &str) -> Result<SubAgentResult> {
        self.ensure_consistency();
        let agent = self
            .agents
            .get(agent_id)
            .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?;
        Ok(agent.snapshot())
    }

    /// Like [`Self::get_result`], but fills structured fields from blackboard / prose fallback.
    pub fn get_result_with_fallback(
        &mut self,
        agent_id: &str,
        workspace: &std::path::Path,
    ) -> Result<SubAgentResult> {
        self.ensure_consistency();
        let agent = self
            .agents
            .get(agent_id)
            .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?;
        let mut snap = agent.snapshot();
        enrich_subagent_result(&mut snap, agent, workspace);
        Ok(snap)
    }

    /// Scratchpad run bound at spawn time (if any).
    pub fn agent_scratchpad_run_id(&mut self, agent_id: &str) -> Result<Option<String>> {
        self.ensure_consistency();
        self.agents
            .get(agent_id)
            .map(|agent| agent.scratchpad_run_id.clone())
            .ok_or_else(|| anyhow!("Agent {agent_id} not found"))
    }

    /// Adaptive join timeout when `agent_wait` / `agent_result` omit `timeout_ms`.
    pub(crate) fn adaptive_wait_timeout_ms_for(&mut self, agent_id: &str) -> Result<u64> {
        self.ensure_consistency();
        let agent = self
            .agents
            .get(agent_id)
            .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?;
        Ok(super::constants::adaptive_wait_timeout_ms(
            u64::try_from(agent.step_timeout.as_millis()).unwrap_or(u64::MAX),
            agent.max_steps,
            agent.steps_taken,
        ))
    }

    /// Max adaptive timeout across the listed agents (multi-wait).
    pub(crate) fn adaptive_wait_timeout_ms_for_ids(&mut self, ids: &[String]) -> Result<u64> {
        ids.iter()
            .map(|id| self.adaptive_wait_timeout_ms_for(id))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .max()
            .ok_or_else(|| anyhow!("No agent ids provided"))
    }

    /// Cancel a running sub-agent.
    pub fn cancel(&mut self, agent_id: &str) -> Result<SubAgentResult> {
        let (snapshot, changed) = {
            let agent = self
                .agents
                .get_mut(agent_id)
                .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?;

            let mut changed = false;
            if agent.status == SubAgentStatus::Running {
                agent.status = SubAgentStatus::Cancelled;
                release_resident_leases_for(&agent.id);
                if let Some(handle) = agent.task_handle.take() {
                    handle.abort();
                }
                changed = true;
            }
            (agent.snapshot(), changed)
        };

        if changed {
            self.persist_state_best_effort();
        }
        Ok(snapshot)
    }

    /// Resume a non-running sub-agent by restarting it with the original assignment.
    pub fn resume(
        &mut self,
        manager_handle: SharedSubAgentManager,
        runtime: SubAgentRuntime,
        agent_id: &str,
    ) -> Result<SubAgentResult> {
        let status = self
            .agents
            .get(agent_id)
            .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?
            .status
            .clone();

        if status == SubAgentStatus::Running {
            let agent = self
                .agents
                .get(agent_id)
                .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?;
            return Ok(agent.snapshot());
        }

        let running = self.running_count();
        if running >= self.max_agents {
            return Err(anyhow!(
                "Sub-agent limit reached (max {}, running {}). Close or wait for an existing agent before resuming. Consider issuing multiple tool calls in one turn (the dispatcher runs them in parallel) for parallel one-shot work.",
                self.max_agents,
                running
            ));
        }

        let snapshot = {
            let agent = self
                .agents
                .get_mut(agent_id)
                .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?;

            let (input_tx, input_rx) = mpsc::unbounded_channel();
            let restarted_at = Instant::now();
            let mut restart_runtime = runtime.clone();
            if !agent.model.trim().is_empty() && agent.model != "unknown" {
                restart_runtime.model.clone_from(&agent.model);
            }
            let step_timeout = restart_runtime.step_timeout;
            let task = SubAgentTask {
                manager_handle,
                runtime: restart_runtime,
                agent_id: agent.id.clone(),
                agent_type: agent.agent_type.clone(),
                prompt: agent.prompt.clone(),
                assignment: agent.assignment.clone(),
                allowed_tools: agent.allowed_tools.clone(),
                started_at: restarted_at,
                max_steps: self.max_steps,
                input_rx,
                task_id: None,
            };
            let handle = spawn_supervised(
                "subagent-task-resume",
                std::panic::Location::caller(),
                run_subagent_task(task),
            );

            agent.status = SubAgentStatus::Running;
            agent.result = None;
            agent.steps_taken = 0;
            agent.structured_verdict = None;
            agent.structured_findings = None;
            agent.structured_findings_parse_failure = None;
            agent.completion_reason = None;
            agent.progress_status = None;
            agent.last_progress_at = restarted_at;
            agent.step_timeout = step_timeout;
            agent.max_steps = self.max_steps;
            agent.started_at = restarted_at;
            agent.input_tx = Some(input_tx);
            agent.task_handle = Some(handle);

            if let Some(event_tx) = runtime.event_tx {
                let _ = event_tx.try_send(Event::AgentSpawned {
                    id: agent.id.clone(),
                    prompt: format!("(resumed) {}", agent.prompt),
                });
            }

            agent.snapshot()
        };
        self.persist_state_best_effort();

        Ok(snapshot)
    }

    /// Send input to a running sub-agent.
    pub fn send_input(&mut self, agent_id: &str, text: String, interrupt: bool) -> Result<()> {
        let agent = self
            .agents
            .get_mut(agent_id)
            .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?;

        if agent.status != SubAgentStatus::Running {
            return Err(anyhow!("Agent {agent_id} is not running"));
        }

        if agent
            .task_handle
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
        {
            return Err(anyhow!(
                "Agent {agent_id} task ended (possible panic); input was not delivered"
            ));
        }

        let tx = agent
            .input_tx
            .as_ref()
            .ok_or_else(|| anyhow!("Agent {agent_id} cannot accept input"))?;

        tx.send(SubAgentInput { text, interrupt })
            .map_err(|_| anyhow!("Failed to send input to agent {agent_id}"))?;

        Ok(())
    }

    /// Update assignment metadata and optionally send immediate guidance.
    pub fn assign(
        &mut self,
        agent_id: &str,
        objective: Option<String>,
        role: Option<String>,
        message: Option<String>,
        interrupt: bool,
    ) -> Result<SubAgentResult> {
        if objective.is_none() && role.is_none() && message.is_none() {
            return Err(anyhow!(
                "Provide at least one of objective, role, or message"
            ));
        }

        if message.is_some() {
            let status = self
                .agents
                .get(agent_id)
                .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?
                .status
                .clone();
            if status != SubAgentStatus::Running {
                return Err(anyhow!(
                    "Agent {agent_id} is not running; cannot deliver assignment message"
                ));
            }
        }

        let mut changed = false;
        let (input_tx, payload) = {
            let agent = self
                .agents
                .get_mut(agent_id)
                .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?;

            let mut assignment_lines = Vec::new();
            if let Some(objective) = objective {
                let objective = objective.trim();
                if objective.is_empty() {
                    return Err(anyhow!("objective cannot be empty"));
                }
                if agent.assignment.objective != objective {
                    agent.assignment.objective = objective.to_string();
                    changed = true;
                }
                assignment_lines.push(format!("- objective: {}", agent.assignment.objective));
            }

            if let Some(role) = role {
                let normalized = normalize_role_alias(&role)
                    .ok_or_else(|| {
                        anyhow!(
                            "Invalid role alias '{role}'. Use: worker, explorer, awaiter, default"
                        )
                    })?
                    .to_string();
                if agent.assignment.role.as_deref() != Some(normalized.as_str()) {
                    agent.assignment.role = Some(normalized.clone());
                    changed = true;
                }
                assignment_lines.push(format!("- role: {normalized}"));
            }

            let mut payload_parts = Vec::new();
            if !assignment_lines.is_empty() && agent.status == SubAgentStatus::Running {
                payload_parts.push(format!(
                    "Assignment updated:\n{}",
                    assignment_lines.join("\n")
                ));
            }
            if let Some(message) = message {
                let message = message.trim();
                if message.is_empty() {
                    return Err(anyhow!("message cannot be empty"));
                }
                payload_parts.push(format!("Coordinator note:\n{message}"));
            }

            let payload = if payload_parts.is_empty() {
                None
            } else {
                Some(payload_parts.join("\n\n"))
            };

            (agent.input_tx.clone(), payload)
        };

        if let Some(payload) = payload {
            let tx = input_tx
                .ok_or_else(|| anyhow!("Agent {agent_id} cannot accept assignment input"))?;
            tx.send(SubAgentInput {
                text: payload,
                interrupt,
            })
            .map_err(|_| anyhow!("Failed to send assignment to agent {agent_id}"))?;
        }

        if changed {
            self.persist_state_best_effort();
        }

        self.get_result(agent_id)
    }

    /// List all agents and their status.
    #[must_use]
    /// Snapshot a single agent and tag it with the manager's
    /// classification. The bare `SubAgent::snapshot` defaults
    /// `from_prior_session` to `false`; only the manager knows the
    /// matching boot id, so listing goes through here.
    fn snapshot_for_listing(&self, agent: &SubAgent) -> SubAgentResult {
        let mut snap = agent.snapshot();
        snap.from_prior_session = self.is_from_prior_session(agent);
        snap
    }

    /// List all agents currently held by the manager, regardless of
    /// session origin. Use [`Self::list_filtered`] in user-facing tool
    /// paths so prior-session agents stay hidden by default (#405).
    pub fn list(&mut self) -> Vec<SubAgentResult> {
        self.ensure_consistency();
        self.agents
            .values()
            .map(|agent| self.snapshot_for_listing(agent))
            .collect()
    }

    /// List agents respecting the session-boundary filter (#405).
    ///
    /// `include_archived = false` (the default for `agent_list`) drops
    /// any prior-session agent that is no longer running. Prior-session
    /// agents that are still `Running` (e.g. interrupted by a process
    /// restart) stay visible — they may matter for ongoing recovery.
    ///
    /// `include_archived = true` returns everything, with the
    /// `from_prior_session` flag on each `SubAgentResult` so the model
    /// can tell active and archived apart at a glance.
    pub fn list_filtered(&mut self, include_archived: bool) -> Vec<SubAgentResult> {
        self.ensure_consistency();
        self.agents
            .values()
            .filter(|agent| {
                if include_archived {
                    return true;
                }
                if agent.status == SubAgentStatus::Running {
                    return true;
                }
                !self.is_from_prior_session(agent)
            })
            .map(|agent| self.snapshot_for_listing(agent))
            .collect()
    }

    /// Clean up completed agents older than the given duration.
    pub fn cleanup(&mut self, max_age: Duration) {
        let before = self.agents.len();
        self.agents.retain(|_, agent| {
            if agent.status == SubAgentStatus::Running {
                true
            } else {
                agent.started_at.elapsed() < max_age
            }
        });
        if self.agents.len() != before {
            self.persist_state_best_effort();
        }
    }

    pub(super) fn update_from_result(&mut self, agent_id: &str, result: SubAgentResult) {
        let mut changed = false;
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.status = result.status;
            agent.assignment = result.assignment;
            agent.result = result.result;
            agent.structured_verdict = result.structured_verdict;
            agent.structured_findings = result.structured_findings;
            agent.completion_reason = result.completion_reason;
            agent.structured_findings_parse_failure = result.structured_findings_parse_failure;
            agent.steps_taken = result.steps_taken;
            agent.task_handle = None;
            changed = true;
        }
        if changed {
            self.persist_state_best_effort();
        }
    }

    pub(super) fn update_failed(&mut self, agent_id: &str, error: String) {
        self.update_failed_with_reason(agent_id, error, None);
    }

    pub(super) fn update_failed_with_reason(
        &mut self,
        agent_id: &str,
        error: String,
        completion_reason: Option<CompletionReason>,
    ) {
        let mut changed = false;
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.status = SubAgentStatus::Failed(error);
            agent.completion_reason = completion_reason;
            release_resident_leases_for(agent_id);
            agent.task_handle = None;
            changed = true;
        }
        if changed {
            self.persist_state_best_effort();
        }
    }

    /// Detect agents whose background task finished without updating status.
    fn ensure_consistency(&mut self) {
        let mut changed = false;
        for agent in self.agents.values_mut() {
            if matches!(agent.status, SubAgentStatus::Running)
                && agent
                    .task_handle
                    .as_ref()
                    .is_some_and(tokio::task::JoinHandle::is_finished)
            {
                agent.status = SubAgentStatus::Failed(
                    "Zombie: task ended without status update".into(),
                );
                release_resident_leases_for(&agent.id);
                agent.task_handle = None;
                changed = true;
            }
        }
        if changed {
            self.persist_state_best_effort();
        }
    }

    /// Periodic maintenance: zombie scan (P2-10). Cheap enough to run every 30s.
    pub(crate) fn run_maintenance(&mut self) {
        self.ensure_consistency();
    }

    /// Update in-memory execution progress for live `agent_list` / disk snapshots.
    pub(crate) fn record_execution_progress(
        &mut self,
        agent_id: &str,
        steps_taken: u32,
        status: &str,
    ) {
        let Some(agent) = self.agents.get_mut(agent_id) else {
            return;
        };
        if agent.status != SubAgentStatus::Running {
            return;
        }
        let step_changed = agent.steps_taken != steps_taken;
        agent.steps_taken = steps_taken;
        agent.last_progress_at = Instant::now();
        agent.progress_status = Some(status.to_string());
        if step_changed {
            self.persist_state_best_effort();
        }
    }
}

