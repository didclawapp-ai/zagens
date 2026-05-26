//! `SubAgentHost` implementation for the live TUI engine.
//!
//! M3 renamed the trait from `SubAgentSpawnPort` → `SubAgentHost` (still
//! available via a `#[deprecated]` alias in
//! `deepseek_core::engine::subagent_port`) and extended the surface with
//! `running_count` (used by `no_tool_uses.rs`) and the clearer
//! `list_with_cleanup` method name. The orchestration logic is unchanged —
//! the trait still impls on `Engine` itself because the spawn path touches
//! `deepseek_client`, `session.model`, `tx_event`, and the subagent
//! manager's cleanup window in lockstep.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use deepseek_core::engine::hosts::SubAgentHost;
use deepseek_core::engine::{SubAgentSpawnError, SubAgentSpawnOutcome};
use deepseek_core::subagent::SubAgentResult;

use crate::agent_surface::AppMode;

use super::Engine;

/// Drop completed sub-agents older than this before op-loop listing.
const SUBAGENT_LIST_CLEANUP_MAX_AGE: Duration = Duration::from_secs(60 * 60);

#[async_trait]
impl SubAgentHost for Engine {
    async fn spawn_general(
        &self,
        prompt: &str,
    ) -> Result<SubAgentSpawnOutcome, SubAgentSpawnError> {
        Engine::spawn_general_subagent(self, prompt).await
    }

    async fn list_with_cleanup(&self) -> Vec<SubAgentResult> {
        Engine::list_subagents(self).await
    }

    async fn running_count(&self) -> usize {
        let mgr = self.runtime_ext().subagent_manager.read().await;
        mgr.running_count()
    }
}

impl Engine {
    pub(in crate::core::engine) async fn spawn_general_subagent(
        &self,
        prompt: &str,
    ) -> Result<SubAgentSpawnOutcome, SubAgentSpawnError> {
        use crate::tools::subagent::{
            SubAgentRuntime, SubAgentType, resolve_subagent_assignment_route,
        };

        let Some(client) = self.deepseek_client.clone() else {
            let message = self
                .deepseek_client_error
                .as_deref()
                .map(|err| format!("Failed to spawn sub-agent: {err}"))
                .unwrap_or_else(|| {
                    "Failed to spawn sub-agent: API client not configured".to_string()
                });
            return Err(if self.deepseek_client_error.is_some() {
                SubAgentSpawnError::SpawnFailed(message)
            } else {
                SubAgentSpawnError::NoClient
            });
        };

        let mut runtime = SubAgentRuntime::new(
            client,
            self.session.model.clone(),
            self.build_tool_context(AppMode::Agent, self.session.auto_approve),
            self.session.allow_shell,
            Some(self.tx_event.clone()),
            Arc::clone(&self.runtime_ext().subagent_manager),
        )
        .with_role_models(self.config.subagent_model_overrides.clone())
        .with_auto_model(self.session.auto_model)
        .with_reasoning_effort(
            self.session.reasoning_effort.clone(),
            self.session.reasoning_effort_auto,
        )
        .with_max_spawn_depth(self.config.max_spawn_depth)
        .with_step_timeout(self.config.subagent_step_timeout)
        .background_runtime();
        let route = resolve_subagent_assignment_route(&runtime, None, prompt).await;
        runtime.model = route.model;
        runtime.reasoning_effort = route.reasoning_effort;
        runtime.reasoning_effort_auto = false;

        let snapshot = {
            let mut manager = self.runtime_ext().subagent_manager.write().await;
            manager
                .spawn_background(
                    Arc::clone(&self.runtime_ext().subagent_manager),
                    runtime,
                    SubAgentType::General,
                    prompt.to_string(),
                    None,
                )
                .map_err(|err| SubAgentSpawnError::SpawnFailed(err.to_string()))?
        };

        Ok(SubAgentSpawnOutcome {
            agent_id: snapshot.agent_id,
        })
    }

    pub(in crate::core::engine) async fn list_subagents(&self) -> Vec<SubAgentResult> {
        let mut manager = self.runtime_ext().subagent_manager.write().await;
        manager.cleanup(SUBAGENT_LIST_CLEANUP_MAX_AGE);
        manager.list()
    }

    pub(in crate::core::engine) async fn handle_spawn_subagent_op(&self, prompt: &str) {
        use crate::core::events::Event;
        use deepseek_core::error_taxonomy::ErrorEnvelope;

        match self.spawn_general_subagent(prompt).await {
            Ok(outcome) => {
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "Spawned sub-agent {}",
                        outcome.agent_id
                    )))
                    .await;
            }
            Err(SubAgentSpawnError::NoClient) => {
                let _ = self
                    .tx_event
                    .send(Event::error(ErrorEnvelope::fatal(
                        "Failed to spawn sub-agent: API client not configured",
                    )))
                    .await;
            }
            Err(SubAgentSpawnError::SpawnFailed(message)) => {
                let _ = self
                    .tx_event
                    .send(Event::error(ErrorEnvelope::fatal(message)))
                    .await;
            }
        }
    }
}
