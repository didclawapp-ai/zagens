//! Engine handle for UI/runtime communication.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use anyhow::Result;
use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::context_snapshot::ThreadContextSnapshot;
use crate::core::events::Event;
use crate::core::ops::Op;
use crate::tools::user_input::UserInputResponse;

use super::approval::{ApprovalDecision, UserInputDecision};

/// Handle to communicate with the engine
#[derive(Clone)]
pub struct EngineHandle {
    /// Send operations to the engine
    pub tx_op: mpsc::Sender<Op>,
    /// Receive events from the engine
    pub rx_event: Arc<RwLock<mpsc::Receiver<Event>>>,
    /// Shared pointer to the cancellation token for the current request.
    pub(super) cancel_token: Arc<StdMutex<CancellationToken>>,
    /// Send approval decisions to the engine
    pub(super) tx_approval: mpsc::Sender<ApprovalDecision>,
    /// Send user input responses to the engine
    pub(super) tx_user_input: mpsc::Sender<UserInputDecision>,
    /// Send steer input for an in-flight turn.
    pub(super) tx_steer: mpsc::Sender<String>,
}

impl EngineHandle {
    /// Send an operation to the engine
    pub async fn send(&self, op: Op) -> Result<()> {
        self.tx_op.send(op).await?;
        Ok(())
    }

    /// Cancel the current request
    pub fn cancel(&self) {
        match self.cancel_token.lock() {
            Ok(token) => token.cancel(),
            Err(poisoned) => poisoned.into_inner().cancel(),
        }
    }

    /// Check if a request is currently cancelled
    #[must_use]
    #[allow(dead_code)]
    pub fn is_cancelled(&self) -> bool {
        match self.cancel_token.lock() {
            Ok(token) => token.is_cancelled(),
            Err(poisoned) => poisoned.into_inner().is_cancelled(),
        }
    }

    /// Approve a pending tool call
    pub async fn approve_tool_call(&self, id: impl Into<String>) -> Result<()> {
        self.tx_approval
            .send(ApprovalDecision::Approved { id: id.into() })
            .await?;
        Ok(())
    }

    /// Deny a pending tool call
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
        policy: crate::sandbox::SandboxPolicy,
    ) -> Result<()> {
        self.tx_approval
            .send(ApprovalDecision::RetryWithPolicy {
                id: id.into(),
                policy,
            })
            .await?;
        Ok(())
    }

    /// Submit a response for request_user_input.
    pub async fn submit_user_input(
        &self,
        id: impl Into<String>,
        response: UserInputResponse,
    ) -> Result<()> {
        self.tx_user_input
            .send(UserInputDecision::Submitted {
                id: id.into(),
                response,
            })
            .await?;
        Ok(())
    }

    /// Cancel a request_user_input prompt.
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
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.send(Op::QueryContext { reply: tx }).await?;
        tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .map_err(|_| anyhow::anyhow!("context query timed out"))?
            .map_err(|_| anyhow::anyhow!("engine dropped context query"))
    }
}
