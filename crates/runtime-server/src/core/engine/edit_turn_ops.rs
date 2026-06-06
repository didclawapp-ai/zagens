//! `/edit` last-turn replacement (P2 — `Op::EditLastTurn`).

use super::*;

impl Engine {
    /// #383 `/edit` — drop the last user+assistant exchange, then re-send.
    pub(super) async fn handle_edit_last_turn(&mut self, new_message: String) {
        let _ =
            deepseek_core::session::truncate_before_last_user_message(&mut self.session.messages);
        self.handle_send_message(
            new_message,
            AppMode::Agent,
            self.session.model.clone(),
            self.config.goal_objective.clone(),
            self.session.reasoning_effort.clone(),
            self.session.reasoning_effort_auto,
            self.session.auto_model,
            self.session.allow_shell,
            self.session.trust_mode,
            self.session.auto_approve,
            self.session.approval_mode,
            self.session.temperature,
            self.session.top_p,
            self.session.max_output_tokens,
        )
        .await;
    }
}
