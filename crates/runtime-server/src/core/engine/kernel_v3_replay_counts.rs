//! Last v3 turn replay effect counts for `GET /v1/runtime/kernel-shadow`.

use std::sync::{Mutex, OnceLock};

use zagens_core::engine::turn_machine::ReplayEffectCounts;

#[derive(Debug, Clone, Default)]
pub struct V3LastReplayEffectCounts {
    pub turn_id: Option<String>,
    pub counts: ReplayEffectCounts,
}

static GLOBAL_V3_REPLAY_COUNTS: OnceLock<Mutex<V3LastReplayEffectCounts>> = OnceLock::new();

fn global_v3_replay_counts() -> &'static Mutex<V3LastReplayEffectCounts> {
    GLOBAL_V3_REPLAY_COUNTS.get_or_init(|| Mutex::new(V3LastReplayEffectCounts::default()))
}

pub fn record_v3_turn_replay_effect_counts(turn_id: &str, counts: ReplayEffectCounts) {
    if let Ok(mut slot) = global_v3_replay_counts().lock() {
        slot.turn_id = Some(turn_id.to_string());
        slot.counts = counts;
    }
}

#[must_use]
pub fn v3_last_replay_effect_counts() -> V3LastReplayEffectCounts {
    global_v3_replay_counts()
        .lock()
        .map(|slot| slot.clone())
        .unwrap_or_default()
}
