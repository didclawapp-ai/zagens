//! Turn loop (P2 PR4): [`handle_deepseek_turn`] in `deepseek-core`; L2 phases here.

mod host_impl;
mod streaming_phase;
mod tool_phase;

use super::Engine;
use host_impl::app_mode_to_turn_loop;

impl Engine {
    #[must_use]
    pub(super) fn messages_with_turn_metadata(&self) -> Vec<crate::models::Message> {
        deepseek_core::engine::messages_with_turn_metadata(&self.session, &self.config.workspace)
    }

    pub(super) async fn handle_deepseek_turn(
        &mut self,
        turn: &mut deepseek_core::turn::TurnContext,
        tool_registry: Option<&crate::tools::ToolRegistry>,
        tools: Option<Vec<crate::models::Tool>>,
        mode: crate::tui::app::AppMode,
        force_update_plan_first: bool,
    ) -> (
        deepseek_core::turn::TurnOutcomeStatus,
        Option<String>,
    ) {
        deepseek_core::engine::handle_deepseek_turn(
            self,
            turn,
            tool_registry,
            tools,
            app_mode_to_turn_loop(mode),
            force_update_plan_first,
        )
        .await
    }
}
