//! Engine handle — outbound channel set used by the UI/runtime to drive the
//! engine (M1 → `deepseek-core`).
//!
//! The handle is generic over:
//! - `P` — sandbox policy carried by `ApprovalDecision::RetryWithPolicy`
//!   (concrete type stays in tui; `EngineHandle::retry_tool_with_policy`
//!   accepts it as opaque).
//! - `R` — payload returned by `request_user_input`
//!   (the tui-side `UserInputResponse` plugs in here).
//!
//! The tui crate provides a type alias
//! `EngineHandle = deepseek_core::engine::handle::EngineHandle<SandboxPolicy, UserInputResponse>`
//! so existing call sites keep working.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio_util::sync::CancellationToken;

use crate::engine::approval::{ApprovalDecision, UserInputDecision};
use crate::engine::context_snapshot::ThreadContextSnapshot;
use crate::engine::op::Op;
use crate::engine::start_turn::StartTurnParams;
use crate::engine::turn_port::TurnEnginePort;
use crate::events::Event;
use crate::turn::TurnLoopMode;

/// Handle to communicate with the engine.
#[derive(Clone)]
pub struct EngineHandle<P, R> {
    /// Send operations to the engine.
    pub tx_op: mpsc::Sender<Op>,
    /// Receive events from the engine.
    pub rx_event: Arc<RwLock<mpsc::Receiver<Event>>>,
    /// Shared pointer to the cancellation token for the current request.
    cancel_token: Arc<StdMutex<CancellationToken>>,
    /// Send approval decisions to the engine.
    tx_approval: mpsc::Sender<ApprovalDecision<P>>,
    /// Send user input responses to the engine.
    tx_user_input: mpsc::Sender<UserInputDecision<R>>,
    /// Send steer input for an in-flight turn.
    tx_steer: mpsc::Sender<String>,
}

impl<P, R> EngineHandle<P, R>
where
    P: Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    /// Construct a new handle. Intended for the engine's bootstrap path; the
    /// returned handle is `Clone`, so the runtime/UI layer copies it cheaply.
    #[must_use]
    pub fn new(
        tx_op: mpsc::Sender<Op>,
        rx_event: Arc<RwLock<mpsc::Receiver<Event>>>,
        cancel_token: Arc<StdMutex<CancellationToken>>,
        tx_approval: mpsc::Sender<ApprovalDecision<P>>,
        tx_user_input: mpsc::Sender<UserInputDecision<R>>,
        tx_steer: mpsc::Sender<String>,
    ) -> Self {
        Self {
            tx_op,
            rx_event,
            cancel_token,
            tx_approval,
            tx_user_input,
            tx_steer,
        }
    }

    /// Send an operation to the engine.
    pub async fn send(&self, op: Op) -> Result<()> {
        self.tx_op.send(op).await?;
        Ok(())
    }

    /// Cancel the current request.
    pub fn cancel(&self) {
        match self.cancel_token.lock() {
            Ok(token) => token.cancel(),
            Err(poisoned) => poisoned.into_inner().cancel(),
        }
    }

    /// Check if a request is currently cancelled.
    #[must_use]
    #[allow(dead_code)]
    pub fn is_cancelled(&self) -> bool {
        match self.cancel_token.lock() {
            Ok(token) => token.is_cancelled(),
            Err(poisoned) => poisoned.into_inner().is_cancelled(),
        }
    }

    /// Approve a pending tool call.
    pub async fn approve_tool_call(&self, id: impl Into<String>) -> Result<()> {
        self.tx_approval
            .send(ApprovalDecision::Approved { id: id.into() })
            .await?;
        Ok(())
    }

    /// Deny a pending tool call.
    pub async fn deny_tool_call(&self, id: impl Into<String>) -> Result<()> {
        self.tx_approval
            .send(ApprovalDecision::Denied { id: id.into() })
            .await?;
        Ok(())
    }

    /// Retry a tool call with an elevated sandbox policy.
    pub async fn retry_tool_with_policy(
        &self,
        id: impl Into<String>,
        policy: P,
    ) -> Result<()> {
        self.tx_approval
            .send(ApprovalDecision::RetryWithPolicy {
                id: id.into(),
                policy,
            })
            .await?;
        Ok(())
    }

    /// Submit a response for `request_user_input`.
    pub async fn submit_user_input(
        &self,
        id: impl Into<String>,
        response: R,
    ) -> Result<()> {
        self.tx_user_input
            .send(UserInputDecision::Submitted {
                id: id.into(),
                response,
            })
            .await?;
        Ok(())
    }

    /// Cancel a `request_user_input` prompt.
    pub async fn cancel_user_input(&self, id: impl Into<String>) -> Result<()> {
        self.tx_user_input
            .send(UserInputDecision::Cancelled { id: id.into() })
            .await?;
        Ok(())
    }

    /// Steer an in-flight turn with additional user input.
    pub async fn steer(&self, content: impl Into<String>) -> Result<()> {
        self.tx_steer.send(content.into()).await?;
        Ok(())
    }

    /// Query TUI-aligned context usage from the live engine session.
    pub async fn query_context_snapshot(&self) -> Result<ThreadContextSnapshot> {
        let (tx, rx) = oneshot::channel();
        self.send(Op::QueryContext { reply: tx }).await?;
        tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .map_err(|_| anyhow::anyhow!("context query timed out"))?
            .map_err(|_| anyhow::anyhow!("engine dropped context query"))
    }

    /// Remove the last user message and everything after it (F4 / `#383`).
    pub async fn truncate_before_last_user_message(&self) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.send(Op::TruncateBeforeLastUserMessage { reply: tx })
            .await?;
        rx.await
            .map_err(|_| anyhow::anyhow!("engine dropped truncate-before-last-user reply"))
    }
}

#[async_trait]
impl<P, R> TurnEnginePort for EngineHandle<P, R>
where
    P: Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    async fn start_turn(&self, params: StartTurnParams) -> Result<()> {
        params.validate().map_err(anyhow::Error::msg)?;
        self.send(Op::SendMessage {
            content: params.prompt,
            mode: TurnLoopMode::from_setting(&params.mode),
            model: params.model,
            goal_objective: None,
            reasoning_effort: params.reasoning_effort,
            reasoning_effort_auto: params.reasoning_effort_auto,
            auto_model: params.auto_model,
            allow_shell: params.allow_shell,
            trust_mode: params.trust_mode,
            auto_approve: params.auto_approve,
            approval_mode: params.approval_mode,
            temperature: params.temperature,
            top_p: params.top_p,
            max_output_tokens: params.max_output_tokens,
        })
        .await
    }

    fn cancel_active_turn(&self) {
        self.cancel();
    }
}
