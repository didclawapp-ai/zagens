//! Shared session / config accessors for outer loop and legacy inner-step IO.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::chat::{LlmClient, Message};
use crate::compaction::CompactionConfig;
use crate::events::Event;
use crate::scratchpad::ScratchpadConfig;
use crate::session::Session;

/// Session plane shared by [`super::turn_loop_outer_host::TurnLoopOuterHost`] and
/// [`super::inner_step_host::InnerStepHost`].
#[async_trait]
pub trait TurnLoopSessionHost: Send {
    fn session_mut(&mut self) -> &mut Session;

    fn compaction_config(&self) -> &CompactionConfig;

    #[must_use]
    fn compaction_enabled(&self) -> bool {
        self.compaction_config().enabled
    }

    fn workspace(&self) -> &Path;

    #[must_use]
    fn strict_tool_mode(&self) -> bool;

    fn scratchpad_config(&self) -> &ScratchpadConfig;

    fn scratchpad_run_id(&self) -> Option<&str>;

    fn scratchpad_summary_injected_mut(&mut self) -> &mut bool;

    fn cancel_token(&self) -> &CancellationToken;

    fn tx_event(&self) -> &mpsc::Sender<Event>;

    fn rx_steer_mut(&mut self) -> &mut mpsc::Receiver<String>;

    fn tool_exec_lock(&self) -> Arc<RwLock<()>>;

    fn llm_client(&self) -> Option<Arc<dyn LlmClient>>;

    async fn add_session_message(&mut self, message: Message);

    async fn emit_session_updated(&mut self);

    fn estimated_input_tokens(&self) -> usize;

    /// BCP-47 locale tag for model-facing system hints (e.g. length continuation).
    #[must_use]
    fn locale_tag(&self) -> &str {
        "en"
    }
}

#[cfg(test)]
mod tests {
    const SESSION_HOST_METHOD_BASELINE: usize = 17;

    #[test]
    fn turn_loop_session_host_method_baseline() {
        let methods = [
            "session_mut",
            "compaction_config",
            "compaction_enabled",
            "workspace",
            "strict_tool_mode",
            "scratchpad_config",
            "scratchpad_run_id",
            "scratchpad_summary_injected_mut",
            "cancel_token",
            "tx_event",
            "rx_steer_mut",
            "tool_exec_lock",
            "llm_client",
            "add_session_message",
            "emit_session_updated",
            "estimated_input_tokens",
            "locale_tag",
        ];
        assert_eq!(methods.len(), SESSION_HOST_METHOD_BASELINE);
    }
}
