//! Thin op-loop handlers for non-turn engine operations (P2).

use crate::context_snapshot::ThreadContextSnapshot;
use crate::core::events::Event;
use crate::tui::app::AppMode;
use tokio::sync::oneshot;

use super::Engine;

impl Engine {
    pub(in crate::core::engine) fn handle_cancel_request_op(&mut self) {
        self.cancel_token.cancel();
        self.reset_cancel_token();
    }

    pub(in crate::core::engine) async fn handle_approve_tool_call_op(&self, id: &str) {
        let _ = self
            .tx_event
            .send(Event::status(format!("Approved tool call: {id}")))
            .await;
    }

    pub(in crate::core::engine) async fn handle_deny_tool_call_op(&self, id: &str) {
        let _ = self
            .tx_event
            .send(Event::status(format!("Denied tool call: {id}")))
            .await;
    }

    pub(in crate::core::engine) async fn handle_list_subagents_op(&self) {
        let agents = self.list_subagents().await;
        let _ = self.tx_event.send(Event::AgentList { agents }).await;
    }

    pub(in crate::core::engine) async fn handle_change_mode_op(&self, mode: AppMode) {
        let _ = self
            .tx_event
            .send(Event::status(format!("Mode changed to: {mode:?}")))
            .await;
    }

    pub(in crate::core::engine) fn handle_query_context_op(
        &self,
        reply: oneshot::Sender<ThreadContextSnapshot>,
    ) {
        let _ = reply.send(self.engine_context_snapshot());
    }

    pub(in crate::core::engine) async fn handle_compact_context_op(&mut self) {
        self.handle_manual_compaction().await;
    }

    pub(in crate::core::engine) async fn handle_rlm_op(
        &mut self,
        content: String,
        model: String,
        child_model: String,
        max_depth: u32,
    ) {
        self.handle_rlm(content, model, child_model, max_depth).await;
    }
}
