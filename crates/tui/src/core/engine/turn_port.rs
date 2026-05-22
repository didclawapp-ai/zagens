//! `TurnEnginePort` implementation for the live TUI engine (P2 PR3).

use async_trait::async_trait;
use deepseek_core::engine::{StartTurnParams, TurnEnginePort};

use super::{EngineHandle, Op};
use crate::tui::app::AppMode;

fn parse_mode(mode: &str) -> AppMode {
    AppMode::from_setting(mode)
}

#[async_trait]
impl TurnEnginePort for EngineHandle {
    async fn start_turn(&self, params: StartTurnParams) -> anyhow::Result<()> {
        params.validate().map_err(anyhow::Error::msg)?;
        self.send(Op::SendMessage {
            content: params.prompt,
            mode: parse_mode(&params.mode),
            model: params.model,
            goal_objective: None,
            reasoning_effort: params.reasoning_effort,
            reasoning_effort_auto: params.reasoning_effort_auto,
            auto_model: params.auto_model,
            allow_shell: params.allow_shell,
            trust_mode: params.trust_mode,
            auto_approve: params.auto_approve,
            approval_mode: params.approval_mode,
        })
        .await
    }

    fn cancel_active_turn(&self) {
        self.cancel();
    }
}
