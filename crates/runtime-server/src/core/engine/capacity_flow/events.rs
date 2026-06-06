//! Capacity and compaction event emission for the engine loop.

use super::super::*;

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
    ) {
        let Some(snapshot) = snapshot else {
            return;
        };
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
    }

    pub(in crate::core::engine) async fn emit_capacity_intervention(
        &mut self,
        turn: &TurnContext,
        action: GuardrailAction,
        before_prompt_tokens: usize,
        after_prompt_tokens: usize,
        replay_outcome: Option<String>,
        replan_performed: bool,
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
            })
            .await;
        self.emit_coherence_signal(
            CoherenceSignal::CapacityIntervention { action },
            format!("capacity_intervention: action={}", action.as_str()),
        )
        .await;
    }
}
