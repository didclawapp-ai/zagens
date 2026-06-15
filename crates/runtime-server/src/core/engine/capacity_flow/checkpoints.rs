//! Pre/post-tool and error-escalation capacity checkpoints.

use std::sync::Arc;

use tokio::sync::{Mutex as AsyncMutex, RwLock};

use crate::mcp::McpPool;
use zagens_core::engine::kernel_event::CapacityCheckpointKind;
use zagens_core::engine::turn_loop::should_run_capacity_error_escalation;
use zagens_core::turn::{TurnContext, TurnLoopMode};

use super::super::*;
use super::v3_routing::CapacityDispatchContext;

impl Engine {
    pub(in crate::core::engine) async fn run_capacity_pre_request_checkpoint(
        &mut self,
        turn: &TurnContext,
        client: Option<&dyn crate::llm_client::LlmClient>,
        mode: TurnLoopMode,
    ) -> bool {
        let observation = self.capacity_observation(turn);
        let snapshot = self.0.capacity_controller.observe_pre_turn(observation);
        let decision = self
            .0
            .capacity_controller
            .decide(self.0.turn_counter, snapshot.as_ref());
        self.emit_capacity_decision(
            turn,
            snapshot.as_ref(),
            &decision,
            CapacityCheckpointKind::PreRequest,
        )
        .await;

        self.dispatch_capacity_decision(CapacityDispatchContext {
            turn,
            mode,
            snapshot: snapshot.as_ref(),
            decision: &decision,
            client,
            tool_registry: None,
            tool_exec_lock: None,
            mcp_pool: None,
            handoff_reason: "capacity_handoff",
        })
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::core::engine) async fn run_capacity_post_tool_checkpoint(
        &mut self,
        turn: &TurnContext,
        mode: TurnLoopMode,
        tool_registry: Option<&crate::tools::ToolRegistry>,
        tool_exec_lock: Arc<RwLock<()>>,
        mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
        _step_error_count: usize,
        _consecutive_tool_error_steps: u32,
    ) -> bool {
        let observation = self.capacity_observation(turn);
        let snapshot = self.0.capacity_controller.observe_post_tool(observation);
        let decision = self
            .0
            .capacity_controller
            .decide(self.0.turn_counter, snapshot.as_ref());
        self.emit_capacity_decision(
            turn,
            snapshot.as_ref(),
            &decision,
            CapacityCheckpointKind::PostTool,
        )
        .await;

        self.dispatch_capacity_decision(CapacityDispatchContext {
            turn,
            mode,
            snapshot: snapshot.as_ref(),
            decision: &decision,
            client: None,
            tool_registry,
            tool_exec_lock: Some(tool_exec_lock),
            mcp_pool,
            handoff_reason: "high_risk_post_tool",
        })
        .await
    }

    pub(in crate::core::engine) async fn run_capacity_error_escalation_checkpoint(
        &mut self,
        turn: &TurnContext,
        mode: TurnLoopMode,
        step_error_count: usize,
        consecutive_tool_error_steps: u32,
        error_categories: &[ErrorCategory],
    ) -> bool {
        if !should_run_capacity_error_escalation(
            step_error_count,
            consecutive_tool_error_steps,
            error_categories,
        ) {
            return false;
        }

        let snapshot = self
            .0
            .capacity_controller
            .last_snapshot()
            .cloned()
            .or_else(|| {
                let observation = self.capacity_observation(turn);
                self.0.capacity_controller.observe_pre_turn(observation)
            });
        let Some(snapshot) = snapshot else {
            return false;
        };

        let repeated_failures = step_error_count >= 2 || consecutive_tool_error_steps >= 2;
        let mut forced = snapshot.clone();
        if repeated_failures && !(snapshot.risk_band == RiskBand::High && snapshot.severe) {
            forced.risk_band = RiskBand::High;
            forced.severe = true;
        }

        let decision = self
            .0
            .capacity_controller
            .decide(self.0.turn_counter, Some(&forced));
        self.emit_capacity_decision(
            turn,
            Some(&forced),
            &decision,
            CapacityCheckpointKind::ErrorEscalation,
        )
        .await;

        let category_labels: Vec<String> = error_categories.iter().map(|c| c.to_string()).collect();
        let handoff_reason = format!(
            "error_escalation: step_errors={}, consecutive_steps={}, categories={}",
            step_error_count,
            consecutive_tool_error_steps,
            category_labels.join(",")
        );
        self.dispatch_capacity_decision(CapacityDispatchContext {
            turn,
            mode,
            snapshot: Some(&forced),
            decision: &decision,
            client: None,
            tool_registry: None,
            tool_exec_lock: None,
            mcp_pool: None,
            handoff_reason: &handoff_reason,
        })
        .await
    }
}
