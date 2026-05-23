//! Pre/post-tool and error-escalation capacity checkpoints.

use std::sync::Arc;

use tokio::sync::{Mutex as AsyncMutex, RwLock};

use crate::mcp::McpPool;
use crate::tui::app::AppMode;

use super::super::*;

impl Engine {
    pub(in crate::core::engine) async fn run_capacity_pre_request_checkpoint(
        &mut self,
        turn: &TurnContext,
        client: Option<&(dyn crate::llm_client::LlmClient)>,
        mode: AppMode,
    ) -> bool {
        let snapshot = self
            .capacity_controller
            .observe_pre_turn(self.capacity_observation(turn));
        let decision = self
            .capacity_controller
            .decide(self.turn_counter, snapshot.as_ref());
        self.emit_capacity_decision(turn, snapshot.as_ref(), &decision)
            .await;

        if decision.action != GuardrailAction::TargetedContextRefresh {
            return false;
        }

        self.apply_targeted_context_refresh(turn, client, mode, snapshot.as_ref())
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::core::engine) async fn run_capacity_post_tool_checkpoint(
        &mut self,
        turn: &TurnContext,
        mode: AppMode,
        tool_registry: Option<&crate::tools::ToolRegistry>,
        tool_exec_lock: Arc<RwLock<()>>,
        mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
        _step_error_count: usize,
        _consecutive_tool_error_steps: u32,
    ) -> bool {
        let snapshot = self
            .capacity_controller
            .observe_post_tool(self.capacity_observation(turn));
        let decision = self
            .capacity_controller
            .decide(self.turn_counter, snapshot.as_ref());
        self.emit_capacity_decision(turn, snapshot.as_ref(), &decision)
            .await;

        match decision.action {
            GuardrailAction::VerifyWithToolReplay => {
                let _ = self
                    .apply_verify_with_tool_replay(
                        turn,
                        mode,
                        snapshot.as_ref(),
                        tool_registry,
                        tool_exec_lock,
                        mcp_pool,
                    )
                    .await;
                false
            }
            GuardrailAction::VerifyAndReplan => {
                self.apply_verify_and_replan(turn, mode, snapshot.as_ref(), "high_risk_post_tool")
                    .await
            }
            GuardrailAction::NoIntervention | GuardrailAction::TargetedContextRefresh => false,
        }
    }

    pub(in crate::core::engine) async fn run_capacity_error_escalation_checkpoint(
        &mut self,
        turn: &TurnContext,
        mode: AppMode,
        step_error_count: usize,
        consecutive_tool_error_steps: u32,
        error_categories: &[ErrorCategory],
    ) -> bool {
        if step_error_count == 0 && consecutive_tool_error_steps < 2 {
            return false;
        }

        // Categorize this step's failures by typed `ErrorCategory` rather than
        // substring-matching error strings. Context overflow always escalates;
        // network / rate-limit / timeout are transient and skip escalation;
        // anything else only escalates with consecutive consecutive failures.
        let has_context_overflow = error_categories.contains(&ErrorCategory::InvalidInput);
        let only_transient = !error_categories.is_empty()
            && error_categories.iter().all(|c| {
                matches!(
                    c,
                    ErrorCategory::Network | ErrorCategory::RateLimit | ErrorCategory::Timeout
                )
            });
        if only_transient && !has_context_overflow {
            return false;
        }
        if !has_context_overflow && consecutive_tool_error_steps < 2 {
            return false;
        }

        let snapshot = self
            .capacity_controller
            .last_snapshot()
            .cloned()
            .or_else(|| {
                self.capacity_controller
                    .observe_pre_turn(self.capacity_observation(turn))
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
            .capacity_controller
            .decide(self.turn_counter, Some(&forced));
        self.emit_capacity_decision(turn, Some(&forced), &decision)
            .await;

        if decision.action != GuardrailAction::VerifyAndReplan {
            return false;
        }

        let category_labels: Vec<String> = error_categories.iter().map(|c| c.to_string()).collect();
        self.apply_verify_and_replan(
            turn,
            mode,
            Some(&forced),
            &format!(
                "error_escalation: step_errors={}, consecutive_steps={}, categories={}",
                step_error_count,
                consecutive_tool_error_steps,
                category_labels.join(",")
            ),
        )
        .await
    }
}
