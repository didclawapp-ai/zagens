//! PR5 — delegate `ThreadMessageTurnPort` to the live sidecar engine path.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use deepseek_core::{
    ThreadMessageTurnPort, ThreadMessageTurnRequest, ThreadMessageTurnResult,
};

use super::types::RuntimeTurnStatus;
use super::{RuntimeThreadManager, StartTurnRequest};

const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(120);

/// Runs a full engine turn via [`RuntimeThreadManager::start_turn`].
pub struct RuntimeThreadMessageTurnPort {
    manager: Arc<RuntimeThreadManager>,
    turn_timeout: Duration,
}

impl RuntimeThreadMessageTurnPort {
    pub fn new(manager: Arc<RuntimeThreadManager>) -> Self {
        Self {
            manager,
            turn_timeout: DEFAULT_TURN_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with_turn_timeout(mut self, timeout: Duration) -> Self {
        self.turn_timeout = timeout;
        self
    }
}

#[async_trait]
impl ThreadMessageTurnPort for RuntimeThreadMessageTurnPort {
    async fn run_turn(
        &self,
        req: ThreadMessageTurnRequest,
    ) -> Result<ThreadMessageTurnResult> {
        let turn = self
            .manager
            .start_turn(
                &req.thread_id,
                StartTurnRequest {
                    prompt: req.input,
                    input_summary: None,
                    model: Some(req.model),
                    mode: None,
                    allow_shell: None,
                    trust_mode: None,
                    auto_approve: Some(true),
                    route_intent: None,
                },
            )
            .await
            .context("start_turn")?;

        let terminal = self
            .manager
            .wait_turn_terminal(&turn.id, self.turn_timeout)
            .await
            .with_context(|| format!("wait turn {}", turn.id))?;

        let status = match terminal.status {
            RuntimeTurnStatus::Completed => "completed".to_string(),
            RuntimeTurnStatus::Failed => "failed".to_string(),
            RuntimeTurnStatus::Interrupted => "interrupted".to_string(),
            RuntimeTurnStatus::Canceled => "canceled".to_string(),
            RuntimeTurnStatus::Queued | RuntimeTurnStatus::InProgress => "in_progress".to_string(),
        };

        let assistant_text = self.manager.assistant_text_for_turn(&terminal)?;
        if assistant_text.trim().is_empty() {
            if let Some(err) = terminal.error.filter(|e| !e.trim().is_empty()) {
                bail!(err);
            }
            bail!("turn completed without assistant output");
        }

        Ok(ThreadMessageTurnResult {
            status,
            assistant_text,
        })
    }
}
