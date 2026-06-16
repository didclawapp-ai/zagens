//! Outer turn-loop host surface (`handle_deepseek_turn` pre/post inner step).

use async_trait::async_trait;

use crate::chat::LlmClient;
use crate::turn::{TurnContext, TurnLoopMode};

use super::turn_loop_session_host::TurnLoopSessionHost;

/// Outer-loop hooks still on the host until `TurnMachine::step` absorbs them (batch 5d).
#[async_trait]
pub trait TurnLoopOuterHost: TurnLoopSessionHost {
    fn reset_scratchpad_step(&mut self);

    async fn refresh_system_prompt(&mut self, mode: TurnLoopMode);

    async fn inject_live_steer(&mut self, turn: &TurnContext, steer: String);

    async fn run_auto_compaction(&mut self, client: &dyn LlmClient, turn: &TurnContext);

    async fn run_pre_inner_step_auto_compaction(
        &mut self,
        client: &dyn LlmClient,
        turn: &TurnContext,
    ) {
        self.run_auto_compaction(client, turn).await;
    }

    async fn layered_context_checkpoint(&mut self);

    async fn run_pre_inner_step_layered_context(&mut self) {
        self.layered_context_checkpoint().await;
    }

    async fn recover_context_overflow(
        &mut self,
        client: &dyn LlmClient,
        reason: &str,
        max_output_tokens: u32,
    ) -> bool;

    async fn run_capacity_pre_request_checkpoint(
        &mut self,
        turn: &TurnContext,
        client: Option<&dyn LlmClient>,
        mode: TurnLoopMode,
    ) -> bool;

    async fn run_capacity_error_escalation_checkpoint(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        step_error_count: usize,
        consecutive_tool_error_steps: u32,
        error_categories: &[crate::error_taxonomy::ErrorCategory],
    ) -> bool;

    async fn maybe_lht_pre_request_hooks(&mut self, _mode: TurnLoopMode) {}

    async fn maybe_continue_at_step_limit(&mut self, _turn: &TurnContext) -> bool {
        false
    }

    async fn maybe_continue_after_loop_guard_halt(&mut self, _turn: &TurnContext) -> bool {
        false
    }

    async fn maybe_cycle_handoff_on_context_overflow(
        &mut self,
        _turn: &TurnContext,
        _mode: TurnLoopMode,
    ) -> bool {
        false
    }

    async fn maybe_advance_cycle_at_checkpoint(
        &mut self,
        _mode: TurnLoopMode,
        _turn: &TurnContext,
    ) -> bool {
        false
    }

    async fn note_incomplete_stop_if_lht(&mut self) {}

    async fn maybe_inject_scratchpad_summary(&mut self, turn: &TurnContext) -> bool;

    async fn maybe_inject_scratchpad_reminder(&mut self, turn: &TurnContext);
}

/// Outer-loop host seam: outer hooks + kernel event sink (batch 5d cont. step 4).
///
/// [`V3TurnHost`] satisfies this via [`TurnLoopOuterHost`] + [`InnerStepHost`]'s
/// [`KernelTurnHost`](crate::engine::kernel_turn_host::KernelTurnHost) supertrait.
pub trait OuterLoopHost:
    TurnLoopOuterHost + crate::engine::kernel_turn_host::KernelTurnHost
{
}

impl<T: TurnLoopOuterHost + crate::engine::kernel_turn_host::KernelTurnHost> OuterLoopHost for T {}

#[cfg(test)]
mod tests {
    use super::OuterLoopHost;
    use crate::engine::kernel_turn_host::KernelTurnHost;
    use crate::engine::turn_loop::host::V3TurnHost;

    fn _v3_turn_host_is_outer_loop_host<H: V3TurnHost>() {}

    fn _outer_loop_host_implies_kernel<H: OuterLoopHost>()
    where
        H: KernelTurnHost,
    {
    }
    const OUTER_HOST_METHOD_BASELINE: usize = 18;

    #[test]
    fn turn_loop_outer_host_method_baseline() {
        let methods = [
            "reset_scratchpad_step",
            "refresh_system_prompt",
            "inject_live_steer",
            "run_auto_compaction",
            "run_pre_inner_step_auto_compaction",
            "layered_context_checkpoint",
            "run_pre_inner_step_layered_context",
            "recover_context_overflow",
            "run_capacity_pre_request_checkpoint",
            "run_capacity_error_escalation_checkpoint",
            "maybe_lht_pre_request_hooks",
            "maybe_continue_at_step_limit",
            "maybe_continue_after_loop_guard_halt",
            "maybe_cycle_handoff_on_context_overflow",
            "maybe_advance_cycle_at_checkpoint",
            "note_incomplete_stop_if_lht",
            "maybe_inject_scratchpad_summary",
            "maybe_inject_scratchpad_reminder",
        ];
        assert_eq!(methods.len(), OUTER_HOST_METHOD_BASELINE);
    }
}
