//! Harness cycles API payload (LHT Phase 3a).

use chrono::{DateTime, Utc};
use deepseek_core::cycle::CycleBriefing;
use serde::Serialize;
use serde_json::{Value, json};

use crate::cycle_manager::CycleArchiveSummary;
use crate::models::context_window_for_model;
use deepseek_core::cycle::DEFAULT_CYCLE_THRESHOLD_TOKENS;

use super::cycle_band::{LHT_WARNING_BAND_HIGH, LHT_WARNING_BAND_LOW};

#[derive(Debug, Clone, Serialize)]
pub struct HarnessCycleBriefingJson {
    pub cycle: u32,
    pub timestamp: DateTime<Utc>,
    pub briefing_preview: String,
    pub token_estimate: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessCycleArchiveJson {
    pub cycle: u32,
    pub started: DateTime<Utc>,
    pub ended: DateTime<Utc>,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessCyclesResponse {
    pub cycle_count: u32,
    pub current_cycle: u32,
    pub briefings: Vec<HarnessCycleBriefingJson>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub archives: Vec<HarnessCycleArchiveJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_pressure_pct: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle_threshold_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lht_warning_low_pct: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lht_warning_high_pct: Option<u8>,
}

const BRIEFING_PREVIEW_CHARS: usize = 280;

#[must_use]
pub fn briefing_to_json(b: &CycleBriefing) -> HarnessCycleBriefingJson {
    let preview = if b.briefing_text.chars().count() <= BRIEFING_PREVIEW_CHARS {
        b.briefing_text.clone()
    } else {
        let mut s: String = b
            .briefing_text
            .chars()
            .take(BRIEFING_PREVIEW_CHARS)
            .collect();
        s.push_str("…");
        s
    };
    HarnessCycleBriefingJson {
        cycle: b.cycle,
        timestamp: b.timestamp,
        briefing_preview: preview,
        token_estimate: b.token_estimate,
    }
}

fn context_meta_for_model(
    model: &str,
    configured_threshold: Option<u32>,
) -> (Option<u32>, Option<u32>, Option<u8>, Option<u8>) {
    let Some(window) = context_window_for_model(model) else {
        return (None, None, None, None);
    };
    let window_u32 = u32::from(window);
    (
        Some(window_u32),
        Some(configured_threshold.unwrap_or(DEFAULT_CYCLE_THRESHOLD_TOKENS as u32)),
        Some((LHT_WARNING_BAND_LOW * 100.0).round() as u8),
        Some((LHT_WARNING_BAND_HIGH * 100.0).round() as u8),
    )
}

#[must_use]
pub fn build_cycles_value(
    cycle_count: u32,
    briefings: &[CycleBriefing],
    archive_summaries: &[CycleArchiveSummary],
    context_pressure_pct: Option<u8>,
    model: Option<&str>,
    configured_threshold_tokens: Option<u32>,
) -> Value {
    let (context_window_tokens, cycle_threshold_tokens, lht_warning_low_pct, lht_warning_high_pct) =
        model
            .map(|m| context_meta_for_model(m, configured_threshold_tokens))
            .unwrap_or((None, None, None, None));
    let briefing_cycles: std::collections::HashSet<u32> =
        briefings.iter().map(|b| b.cycle).collect();
    let archives: Vec<HarnessCycleArchiveJson> = archive_summaries
        .iter()
        .filter(|s| !briefing_cycles.contains(&s.cycle))
        .map(|s| HarnessCycleArchiveJson {
            cycle: s.cycle,
            started: s.started,
            ended: s.ended,
            message_count: s.message_count,
        })
        .collect();
    serde_json::to_value(HarnessCyclesResponse {
        cycle_count,
        current_cycle: cycle_count.saturating_add(1),
        briefings: briefings.iter().map(briefing_to_json).collect(),
        archives,
        context_pressure_pct,
        context_window_tokens,
        cycle_threshold_tokens,
        lht_warning_low_pct,
        lht_warning_high_pct,
    })
    .unwrap_or_else(|_| json!({ "error": "cycles_serialize_failed" }))
}
