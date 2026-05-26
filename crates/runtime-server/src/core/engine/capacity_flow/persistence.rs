//! Capacity memory record build/persist and session rehydration.

use super::super::*;

impl Engine {
    pub(in crate::core::engine) fn capacity_source_message_ids(&self, turn: &TurnContext) -> Vec<String> {
        let mut ids: Vec<String> = turn
            .tool_calls
            .iter()
            .rev()
            .take(8)
            .map(|call| call.id.clone())
            .collect();
        ids.reverse();
        ids
    }

    pub(in crate::core::engine) fn build_capacity_record(
        &self,
        turn: &TurnContext,
        action: GuardrailAction,
        snapshot: Option<&CapacitySnapshot>,
        canonical: CanonicalState,
        source_message_ids: Vec<String>,
        replay_info: Option<ReplayInfo>,
    ) -> CapacityMemoryRecord {
        let (h_hat, c_hat, slack, risk_band) = snapshot
            .map(|s| (s.h_hat, s.c_hat, s.slack, s.risk_band.as_str().to_string()))
            .unwrap_or_else(|| (0.0, 0.0, 0.0, "unknown".to_string()));

        CapacityMemoryRecord {
            id: new_record_id(),
            ts: now_rfc3339(),
            turn_index: self.turn_counter,
            action_trigger: action.as_str().to_string(),
            h_hat,
            c_hat,
            slack,
            risk_band,
            canonical_state: canonical,
            source_message_ids: if source_message_ids.is_empty() {
                vec![turn.id.clone()]
            } else {
                source_message_ids
            },
            replay_info,
        }
    }

    pub(in crate::core::engine) async fn persist_capacity_record(
        &mut self,
        turn: &TurnContext,
        action: GuardrailAction,
        record: &CapacityMemoryRecord,
    ) -> String {
        let pointer = format!("memory://{}/{}", self.session.id, record.id);
        if let Err(err) = append_capacity_record(&self.session.id, record) {
            let _ = self
                .tx_event
                .send(Event::CapacityMemoryPersistFailed {
                    session_id: self.session.id.clone(),
                    turn_id: turn.id.clone(),
                    action: action.as_str().to_string(),
                    error: summarize_text(&err.to_string(), 280),
                })
                .await;
            return format!("{pointer}?persist=failed");
        }
        pointer
    }

    pub(in crate::core::engine) fn rehydrate_latest_canonical_state(&mut self) {
        let Ok(records) = load_last_k_capacity_records(&self.session.id, 1) else {
            return;
        };
        let Some(last) = records.last() else {
            return;
        };
        let pointer = format!("memory://{}/{}", self.session.id, last.id);
        let prompt = self.canonical_prompt(
            &last.canonical_state,
            &pointer,
            GuardrailAction::NoIntervention,
            Some("Rehydrated canonical state from memory."),
        );
        self.merge_compaction_summary(Some(prompt));
    }
}
