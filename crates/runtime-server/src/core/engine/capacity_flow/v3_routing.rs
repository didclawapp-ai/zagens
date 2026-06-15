//! v3 capacity intervention routing — trim/handoff/replay/cooldown via effect plan.

use std::sync::Arc;

use tokio::sync::{Mutex as AsyncMutex, RwLock};

use crate::mcp::McpPool;
use crate::tools::ToolRegistry;
use zagens_core::capacity::{CapacityDecision, CapacitySnapshot, GuardrailAction};
use zagens_core::engine::turn_machine::{Effect, capacity_cooldown_backoff_millis};
use zagens_core::turn::{TurnContext, TurnLoopMode};

use super::super::compaction_ops::RunCompactionScope;
use super::super::effect_interpreter::EffectInterpreter;
use super::super::*;

/// Inputs for routing a capacity controller decision to live IO (legacy or v3 effects).
pub(in crate::core::engine) struct CapacityDispatchContext<'a> {
    pub turn: &'a TurnContext,
    pub mode: TurnLoopMode,
    pub snapshot: Option<&'a CapacitySnapshot>,
    pub decision: &'a CapacityDecision,
    pub client: Option<&'a dyn crate::llm_client::LlmClient>,
    pub tool_registry: Option<&'a ToolRegistry>,
    pub tool_exec_lock: Option<Arc<RwLock<()>>>,
    pub mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
    pub handoff_reason: &'a str,
}

impl Engine {
    /// Route a post-`emit_capacity_decision` intervention (skips when `cooldown_blocked`).
    pub(in crate::core::engine) async fn dispatch_capacity_decision(
        &mut self,
        ctx: CapacityDispatchContext<'_>,
    ) -> bool {
        if ctx.decision.cooldown_blocked {
            return false;
        }
        match ctx.decision.action {
            GuardrailAction::TargetedContextRefresh => {
                self.route_capacity_trim_refresh(ctx.turn, ctx.client, ctx.mode, ctx.snapshot)
                    .await
            }
            GuardrailAction::VerifyAndReplan => {
                self.route_capacity_handoff_replan(
                    ctx.turn,
                    ctx.mode,
                    ctx.snapshot,
                    ctx.handoff_reason,
                )
                .await
            }
            GuardrailAction::VerifyWithToolReplay => {
                let Some(registry) = ctx.tool_registry else {
                    return false;
                };
                let Some(lock) = ctx.tool_exec_lock.clone() else {
                    return false;
                };
                let _ = self
                    .route_capacity_tool_replay(
                        ctx.turn,
                        ctx.mode,
                        ctx.snapshot,
                        Some(registry),
                        lock,
                        ctx.mcp_pool.clone(),
                    )
                    .await;
                false
            }
            GuardrailAction::NoIntervention => false,
        }
    }

    /// Route capacity targeted refresh (trim) through v3 `RunCompaction` or legacy IO.
    pub(in crate::core::engine) async fn route_capacity_trim_refresh(
        &mut self,
        turn: &TurnContext,
        client: Option<&dyn crate::llm_client::LlmClient>,
        mode: TurnLoopMode,
        snapshot: Option<&CapacitySnapshot>,
    ) -> bool {
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            return self
                .apply_targeted_context_refresh(turn, client, mode, snapshot)
                .await;
        }
        tracing::info!(
            target: "kernel_v3",
            turn_id = %turn.id,
            step = turn.step,
            "v3 capacity: RunCompaction trim (effect plan)"
        );
        let ext = self.runtime_ext_mut();
        ext.kernel_run_compaction_scope = Some(RunCompactionScope::CapacityTrim);
        ext.kernel_capacity_snapshot = snapshot.cloned();
        ext.kernel_capacity_turn_mode = Some(mode);
        ext.kernel_capacity_handoff_reason = None;
        ext.kernel_capacity_intervention_ok = None;
        let mut interpreter = EffectInterpreter::new(self);
        let _ = interpreter.interpret(Effect::RunCompaction).await;
        self.runtime_ext_mut()
            .kernel_capacity_intervention_ok
            .take()
            .unwrap_or(false)
    }

    /// Route capacity verify-and-replan (handoff) through v3 `RunCompaction` or legacy IO.
    pub(in crate::core::engine) async fn route_capacity_handoff_replan(
        &mut self,
        turn: &TurnContext,
        mode: TurnLoopMode,
        snapshot: Option<&CapacitySnapshot>,
        reason: &str,
    ) -> bool {
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            return self
                .apply_verify_and_replan(turn, mode, snapshot, reason)
                .await;
        }
        tracing::info!(
            target: "kernel_v3",
            turn_id = %turn.id,
            step = turn.step,
            reason,
            "v3 capacity: RunCompaction handoff (effect plan)"
        );
        let ext = self.runtime_ext_mut();
        ext.kernel_run_compaction_scope = Some(RunCompactionScope::CapacityHandoff);
        ext.kernel_capacity_snapshot = snapshot.cloned();
        ext.kernel_capacity_turn_mode = Some(mode);
        ext.kernel_capacity_handoff_reason = Some(reason.to_string());
        ext.kernel_capacity_intervention_ok = None;
        let mut interpreter = EffectInterpreter::new(self);
        let _ = interpreter.interpret(Effect::RunCompaction).await;
        self.runtime_ext_mut()
            .kernel_capacity_intervention_ok
            .take()
            .unwrap_or(false)
    }

    /// Route high-risk post-tool replay through v3 observability path or legacy IO.
    ///
    /// Tool replay keeps the borrow-scoped `tool_registry` on the caller stack (no stash);
    /// v3 mode adds anchor-only gating consistent with other capacity routes.
    pub(in crate::core::engine) async fn route_capacity_tool_replay(
        &mut self,
        turn: &TurnContext,
        mode: TurnLoopMode,
        snapshot: Option<&CapacitySnapshot>,
        tool_registry: Option<&ToolRegistry>,
        tool_exec_lock: Arc<RwLock<()>>,
        mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
    ) -> bool {
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            return self
                .apply_verify_with_tool_replay(
                    turn,
                    mode,
                    snapshot,
                    tool_registry,
                    tool_exec_lock,
                    mcp_pool,
                )
                .await;
        }
        tracing::info!(
            target: "kernel_v3",
            turn_id = %turn.id,
            step = turn.step,
            "v3 capacity: tool replay (effect plan)"
        );
        if self.effect_replay_anchor_only() {
            tracing::info!(
                target: "kernel_v3",
                turn_id = %turn.id,
                "replay anchor-only: skipping capacity tool replay IO"
            );
            return true;
        }
        self.apply_verify_with_tool_replay(
            turn,
            mode,
            snapshot,
            tool_registry,
            tool_exec_lock,
            mcp_pool,
        )
        .await
    }

    /// v3: interpret symbolic cooldown back-off after a blocked capacity decision.
    pub(in crate::core::engine) async fn route_capacity_cooldown_sleep(&mut self) {
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            return;
        }
        let mut interpreter = EffectInterpreter::new(self);
        let _ = interpreter
            .interpret(Effect::Sleep {
                millis: capacity_cooldown_backoff_millis(),
            })
            .await;
    }
}
