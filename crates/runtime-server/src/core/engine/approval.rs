//! Approval + user-input handshake — TUI shell over `zagens-core::engine::approval`.

use crate::core::events::Event;
use crate::tools::approval_cache::{ApprovalCacheStatus, ApprovalKey};
use crate::tools::user_input::{UserInputRequest, UserInputResponse};

use zagens_core::engine::approval::{
    ApprovalDecision as CoreApprovalDecision, ApprovalResult as CoreApprovalResult,
    UserInputDecision as CoreUserInputDecision, recv_user_input_for_tool,
};

use super::Engine;

pub(super) type ApprovalDecision = CoreApprovalDecision<crate::sandbox::SandboxPolicy>;
pub(super) type ApprovalResult = CoreApprovalResult<crate::sandbox::SandboxPolicy>;
pub(super) type UserInputDecision = CoreUserInputDecision<UserInputResponse>;

impl Engine {
    pub(super) async fn await_tool_approval(
        &mut self,
        tool_id: &str,
    ) -> Result<ApprovalResult, crate::tools::spec::ToolError> {
        loop {
            tokio::select! {
                _ = self.0.cancel_token.cancelled() => {
                    return Err(crate::tools::spec::ToolError::execution_failed(
                        "Request cancelled while awaiting approval".to_string(),
                    ));
                }
                decision = self.0.rx_approval.recv() => {
                    let Some(decision) = decision else {
                        return Err(crate::tools::spec::ToolError::execution_failed(
                            "Approval channel closed".to_string(),
                        ));
                    };
                    match decision {
                        ApprovalDecision::Approved {
                            id,
                            cache_key,
                            remember_for_session,
                        } if id == tool_id => {
                            if remember_for_session
                                && let Some(key) = cache_key {
                                    self.runtime_ext_mut()
                                        .approval_cache
                                        .insert(ApprovalKey(key), true);
                                }
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

    pub(super) fn approval_cache_hit(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
    ) -> bool {
        let key = crate::tools::approval_cache::build_approval_key(tool_name, tool_input);
        matches!(
            self.runtime_ext().approval_cache.check(&key),
            ApprovalCacheStatus::Approved
        )
    }

    pub(super) async fn await_user_input(
        &mut self,
        tool_id: &str,
        request: UserInputRequest,
    ) -> Result<UserInputResponse, crate::tools::spec::ToolError> {
        let _ = self
            .tx_event
            .send(Event::UserInputRequired {
                id: tool_id.to_string(),
                request,
            })
            .await;

        recv_user_input_for_tool(tool_id, &self.0.cancel_token, &mut self.0.rx_user_input).await
    }
}
