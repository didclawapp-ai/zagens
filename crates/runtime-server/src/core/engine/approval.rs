//! Approval + user-input handshake — TUI shell over `deepseek-core::engine::approval`.

use crate::core::events::Event;
use crate::tools::user_input::{UserInputRequest, UserInputResponse};

use deepseek_core::engine::approval::{
    await_tool_approval as core_await_tool_approval, recv_user_input_for_tool,
    ApprovalDecision as CoreApprovalDecision, ApprovalResult as CoreApprovalResult,
    UserInputDecision as CoreUserInputDecision,
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
        core_await_tool_approval(tool_id, &self.0.cancel_token, &mut self.0.rx_approval).await
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
