//! Capacity and compaction event emission for the engine loop.

use zagens_core::engine::kernel_event::{CapacityAction, CapacityCheckpointKind, KernelEvent};
use zagens_core::engine::turn_machine::emit_kernel_event;
use zagens_core::turn::TurnContext;

use super::super::*;

fn capacity_metric_as_u32(value: f64, field: &'static str) -> u32 {
    if value.is_nan() {
        tracing::warn!(
            target: "capacity",
            field,
            "capacity token metric is NaN; treating as 0"
        );
        return 0;
    }
    if value > f64::from(u32::MAX) {
        tracing::warn!(
            target: "capacity",
            field,
            raw = value,
            clamped = u32::MAX,
            "capacity token metric exceeds u32::MAX; clamping"
        );
    } else if value < 0.0 {
        tracing::debug!(
            target: "capacity",
            field,
            raw = value,
            "capacity token metric is negative; clamping to 0"
        );
    }
    value.max(0.0).min(f64::from(u32::MAX)) as u32
}

impl Engine {
    pub(in crate::core::engine) async fn emit_coherence_signal(
        &mut self,
        signal: CoherenceSignal,
        reason: impl Into<String>,
    ) {
        let next = next_coherence_state(self.coherence_state, signal);
        self.coherence_state = next;
        let _ = self
            .tx_event
            .send(Event::CoherenceState {
                state: next,
                label: next.label().to_string(),
                description: next.description().to_string(),
                reason: reason.into(),
            })
            .await;
    }

    pub(in crate::core::engine) async fn emit_compaction_started(
        &mut self,
        id: String,
        auto: bool,
        message: String,
    ) {
        let _ = self
            .tx_event
            .send(Event::CompactionStarted {
                id,
                auto,
                message: message.clone(),
            })
            .await;
        self.emit_coherence_signal(CoherenceSignal::CompactionStarted, message)
            .await;
    }

    pub(in crate::core::engine) async fn emit_compaction_completed(
        &mut self,
        id: String,
        auto: bool,
        message: String,
        messages_before: Option<usize>,
        messages_after: Option<usize>,
    ) {
        let _ = self
            .tx_event
            .send(Event::CompactionCompleted {
                id,
                auto,
                message: message.clone(),
                messages_before,
                messages_after,
            })
            .await;
        self.emit_coherence_signal(CoherenceSignal::CompactionCompleted, message)
            .await;
    }

    pub(in crate::core::engine) async fn emit_compaction_failed(
        &mut self,
        id: String,
        auto: bool,
        message: String,
    ) {
        let _ = self
            .tx_event
            .send(Event::CompactionFailed {
                id,
                auto,
                message: message.clone(),
            })
            .await;
        self.emit_coherence_signal(CoherenceSignal::CompactionFailed, message)
            .await;
    }

    pub(in crate::core::engine) async fn emit_capacity_decision(
        &mut self,
        turn: &TurnContext,
        snapshot: Option<&CapacitySnapshot>,
        decision: &CapacityDecision,
        kind: CapacityCheckpointKind,
    ) {
        let Some(snapshot) = snapshot else {
            return;
        };
        let kernel_action = CapacityAction::from_guardrail(decision.action, &decision.reason);
        let tokens_used = capacity_metric_as_u32(snapshot.h_hat, "tokens_used");
        let token_budget = capacity_metric_as_u32(snapshot.c_hat, "token_budget");
        emit_kernel_event(
            self,
            KernelEvent::CapacityCheckpoint {
                turn_id: turn.id.clone(),
                step_idx: turn.step,
                kind,
                tokens_used,
                token_budget,
                action: kernel_action.clone(),
                cooldown_blocked: decision.cooldown_blocked,
            },
        );
        let _ = self
            .tx_event
            .send(Event::CapacityDecision {
                session_id: self.session.id.clone(),
                turn_id: turn.id.clone(),
                h_hat: snapshot.h_hat,
                c_hat: snapshot.c_hat,
                slack: snapshot.slack,
                min_slack: snapshot.profile.min_slack,
                violation_ratio: snapshot.profile.violation_ratio,
                p_fail: snapshot.p_fail,
                risk_band: snapshot.risk_band.as_str().to_string(),
                action: decision.action.as_str().to_string(),
                cooldown_blocked: decision.cooldown_blocked,
                reason: decision.reason.clone(),
            })
            .await;
        self.emit_coherence_signal(
            CoherenceSignal::CapacityDecision {
                risk_band: snapshot.risk_band,
                action: decision.action,
                cooldown_blocked: decision.cooldown_blocked,
            },
            format!(
                "capacity_decision: risk={} action={} reason={}",
                snapshot.risk_band.as_str(),
                decision.action.as_str(),
                decision.reason
            ),
        )
        .await;
        if decision.cooldown_blocked {
            self.route_capacity_cooldown_sleep().await;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::core::engine) async fn emit_capacity_intervention(
        &mut self,
        turn: &TurnContext,
        action: GuardrailAction,
        before_prompt_tokens: usize,
        after_prompt_tokens: usize,
        replay_outcome: Option<String>,
        replan_performed: bool,
        fallback_chain: Option<Vec<String>>,
    ) {
        let _ = self
            .tx_event
            .send(Event::CapacityIntervention {
                session_id: self.session.id.clone(),
                turn_id: turn.id.clone(),
                action: action.as_str().to_string(),
                before_prompt_tokens,
                after_prompt_tokens,
                compaction_size_reduction: before_prompt_tokens.saturating_sub(after_prompt_tokens),
                replay_outcome,
                replan_performed,
                fallback_chain: fallback_chain.clone(),
            })
            .await;
        let chain_note = fallback_chain
            .as_ref()
            .map(|c| c.join("→"))
            .unwrap_or_default();
        let detail = if chain_note.is_empty() {
            format!("capacity_intervention: action={}", action.as_str())
        } else {
            format!(
                "capacity_intervention: action={} chain={chain_note}",
                action.as_str()
            )
        };
        self.emit_coherence_signal(CoherenceSignal::CapacityIntervention { action }, detail)
            .await;
    }
}
