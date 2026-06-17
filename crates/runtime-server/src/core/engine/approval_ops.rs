//! v3 tool approval — routes approval handshake through [`Effect::RequestApproval`].

use serde_json::json;
use zagens_core::chat::ToolCaller;
use zagens_core::engine::dispatch::caller_type_for_tool_use;
use zagens_core::engine::emit_tool_audit;
use zagens_core::engine::kernel_event::{ApprovalVerdict, KernelEvent};
use zagens_core::engine::turn_machine::emit_kernel_event;

use super::approval::ApprovalResult;
use super::*;
use crate::core::events::Event;

/// Per-call approval outcome stashed before v3 `ExecuteBatch` (consumed by `tool_plans_exec`).
#[derive(Debug, Clone)]
pub enum V3ApprovalStepOutcome {
    Approved,
    Denied,
    RetryWithPolicy(crate::sandbox::SandboxPolicy),
    Error(zagens_tools::ToolError),
}

impl Engine {
    pub(in crate::core::engine) fn clear_v3_approval_outcomes(&mut self) {
        self.runtime_ext_mut().kernel_v3_approval_outcomes.clear();
    }

    pub(in crate::core::engine) fn v3_approval_outcome_for(
        &self,
        call_id: &str,
    ) -> Option<V3ApprovalStepOutcome> {
        self.runtime_ext()
            .kernel_v3_approval_outcomes
            .get(call_id)
            .cloned()
    }

    pub(in crate::core::engine) fn routes_tool_approval_via_v3_effect(&self) -> bool {
        self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop()
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::core::engine) async fn run_request_approval_effect(
        &mut self,
        turn_id: &str,
        call_id: &str,
        tool_name: &str,
        tool_input: &serde_json::Value,
        description: &str,
        caller: Option<&ToolCaller>,
    ) -> V3ApprovalStepOutcome {
        let stash = |engine: &mut Self, outcome: V3ApprovalStepOutcome| {
            engine
                .runtime_ext_mut()
                .kernel_v3_approval_outcomes
                .insert(call_id.to_string(), outcome.clone());
            outcome
        };

        if self.effect_replay_anchor_only() {
            tracing::info!(
                target: "kernel_v3",
                call_id = %call_id,
                "replay anchor-only: skipping RequestApproval IO"
            );
            return stash(self, V3ApprovalStepOutcome::Approved);
        }

        if self.approval_cache_hit(tool_name, tool_input) {
            emit_tool_audit(json!({
                "event": "tool.approval_cache_hit",
                "tool_id": call_id,
                "tool_name": tool_name,
            }));
            return stash(self, V3ApprovalStepOutcome::Approved);
        }

        emit_tool_audit(json!({
            "event": "tool.approval_required",
            "tool_id": call_id,
            "tool_name": tool_name,
        }));
        let approval_key =
            crate::tools::approval_cache::build_approval_key(tool_name, tool_input).0;
        let _ = self
            .tx_event
            .send(Event::ApprovalRequired {
                id: call_id.to_string(),
                tool_name: tool_name.to_string(),
                description: description.to_string(),
                approval_key,
            })
            .await;

        let outcome = match self.await_tool_approval(call_id).await {
            Ok(ApprovalResult::Approved { .. }) => {
                emit_tool_audit(json!({
                    "event": "tool.approval_decision",
                    "tool_id": call_id,
                    "tool_name": tool_name,
                    "decision": "approved",
                    "caller": caller_type_for_tool_use(caller),
                }));
                emit_kernel_event(
                    self,
                    KernelEvent::ApprovalResolved {
                        turn_id: turn_id.to_string(),
                        call_id: call_id.to_string(),
                        verdict: ApprovalVerdict::Approved,
                    },
                );
                V3ApprovalStepOutcome::Approved
            }
            Ok(ApprovalResult::Denied) => {
                emit_tool_audit(json!({
                    "event": "tool.approval_decision",
                    "tool_id": call_id,
                    "tool_name": tool_name,
                    "decision": "denied",
                    "caller": caller_type_for_tool_use(caller),
                }));
                emit_kernel_event(
                    self,
                    KernelEvent::ApprovalResolved {
                        turn_id: turn_id.to_string(),
                        call_id: call_id.to_string(),
                        verdict: ApprovalVerdict::Rejected,
                    },
                );
                V3ApprovalStepOutcome::Denied
            }
            Ok(ApprovalResult::RetryWithPolicy(policy)) => {
                emit_tool_audit(json!({
                    "event": "tool.approval_decision",
                    "tool_id": call_id,
                    "tool_name": tool_name,
                    "decision": "retry_with_policy",
                    "policy": format!("{policy:?}"),
                    "caller": caller_type_for_tool_use(caller),
                }));
                emit_kernel_event(
                    self,
                    KernelEvent::ApprovalResolved {
                        turn_id: turn_id.to_string(),
                        call_id: call_id.to_string(),
                        verdict: ApprovalVerdict::Retried,
                    },
                );
                V3ApprovalStepOutcome::RetryWithPolicy(policy)
            }
            Err(err) => V3ApprovalStepOutcome::Error(err),
        };
        stash(self, outcome)
    }
}
