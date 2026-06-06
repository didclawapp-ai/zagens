//! Tool-approval and user-input handshake for the agent loop (P2 PR4 → `deepseek-core`).
//!
//! TUI/Desktop supply policy type `P` (e.g. `SandboxPolicy`) and user response type `R`
//! (e.g. `UserInputResponse`). Event emission for `request_user_input` stays in the L2 shell.

use deepseek_tools::ToolError;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub enum ApprovalDecision<P> {
    Approved {
        id: String,
        /// Fingerprint for session-scoped approval cache (runtime-server).
        cache_key: Option<String>,
        /// When true, identical tool calls skip future prompts for this engine session.
        remember_for_session: bool,
    },
    Denied {
        id: String,
    },
    /// Retry a tool with an elevated sandbox policy.
    RetryWithPolicy {
        id: String,
        policy: P,
    },
}

#[derive(Debug, Clone)]
pub enum UserInputDecision<R> {
    Submitted { id: String, response: R },
    Cancelled { id: String },
}

/// Result of awaiting tool approval from the user.
#[derive(Debug)]
pub enum ApprovalResult<P> {
    Approved,
    Denied,
    RetryWithPolicy(P),
}

/// Block until the user approves, denies, or retries with a new policy for `tool_id`.
pub async fn await_tool_approval<P>(
    tool_id: &str,
    cancel_token: &CancellationToken,
    rx_approval: &mut mpsc::Receiver<ApprovalDecision<P>>,
) -> Result<ApprovalResult<P>, ToolError>
where
    P: Clone,
{
    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                return Err(ToolError::execution_failed(
                    "Request cancelled while awaiting approval".to_string(),
                ));
            }
            decision = rx_approval.recv() => {
                let Some(decision) = decision else {
                    return Err(ToolError::execution_failed(
                        "Approval channel closed".to_string(),
                    ));
                };
                match decision {
                    ApprovalDecision::Approved {
                        id,
                        cache_key: _,
                        remember_for_session: _,
                    } if id == tool_id => {
                        return Ok(ApprovalResult::Approved);
                    }
                    ApprovalDecision::Denied { id } if id == tool_id => {
                        return Ok(ApprovalResult::Denied);
                    }
                    ApprovalDecision::RetryWithPolicy { id, policy } if id == tool_id => {
                        return Ok(ApprovalResult::RetryWithPolicy(policy));
                    }
                    _ => continue,
                }
            }
        }
    }
}

/// Block until the user submits or cancels input for `tool_id` (after the shell emits the prompt).
pub async fn recv_user_input_for_tool<R>(
    tool_id: &str,
    cancel_token: &CancellationToken,
    rx_user_input: &mut mpsc::Receiver<UserInputDecision<R>>,
) -> Result<R, ToolError> {
    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                return Err(ToolError::execution_failed(
                    "Request cancelled while awaiting user input".to_string(),
                ));
            }
            decision = rx_user_input.recv() => {
                let Some(decision) = decision else {
                    return Err(ToolError::execution_failed(
                        "User input channel closed".to_string(),
                    ));
                };
                match decision {
                    UserInputDecision::Submitted { id, response } if id == tool_id => {
                        return Ok(response);
                    }
                    UserInputDecision::Cancelled { id } if id == tool_id => {
                        return Err(ToolError::execution_failed(
                            "User input cancelled".to_string(),
                        ));
                    }
                    _ => continue,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestPolicy(u8);

    #[tokio::test]
    async fn await_tool_approval_matches_id() {
        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(4);
        let tool_id = "tool-1";
        let task = tokio::spawn({
            let cancel = cancel.clone();
            async move { await_tool_approval::<TestPolicy>(tool_id, &cancel, &mut rx).await }
        });
        tx.send(ApprovalDecision::Denied { id: "other".into() })
            .await
            .unwrap();
        tx.send(ApprovalDecision::Approved {
            id: tool_id.into(),
            cache_key: None,
            remember_for_session: false,
        })
        .await
        .unwrap();
        assert!(matches!(
            task.await.unwrap().unwrap(),
            ApprovalResult::Approved
        ));
    }

    #[tokio::test]
    async fn recv_user_input_for_tool_returns_response() {
        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(4);
        let tool_id = "inp-1";
        let task = tokio::spawn({
            let cancel = cancel.clone();
            async move { recv_user_input_for_tool(tool_id, &cancel, &mut rx).await }
        });
        tx.send(UserInputDecision::Submitted {
            id: tool_id.into(),
            response: 42u32,
        })
        .await
        .unwrap();
        assert_eq!(task.await.unwrap().unwrap(), 42);
    }
}
