//! Lightweight evaluation metrics persisted beside the graph (B2.5).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Sidecar metrics file: `metrics.json` in the same directory as the graph file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TopicMemoryMetrics {
    pub turn_updates: u64,
    pub inject_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_inject_at: Option<String>,
    pub clarification_rounds: u64,
    pub repeat_topic_turns: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_user_topics: Option<Vec<String>>,
}

#[must_use]
pub fn metrics_path_for_graph(graph_path: &Path) -> PathBuf {
    graph_path
        .parent()
        .map(|p| p.join("metrics.json"))
        .unwrap_or_else(|| PathBuf::from("metrics.json"))
}

#[must_use]
pub fn load_metrics(path: &Path) -> TopicMemoryMetrics {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist metrics atomically (best-effort).
pub fn save_metrics(path: &Path, metrics: &TopicMemoryMetrics) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(metrics)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(tmp, path)?;
    Ok(())
}

/// Record a turn update; bumps clarification counters when topics repeat across turns.
pub fn record_turn_update(metrics: &mut TopicMemoryMetrics, user_topics: &[String]) {
    metrics.turn_updates = metrics.turn_updates.saturating_add(1);

    if let Some(prev) = metrics.last_user_topics.as_ref() {
        let overlap = user_topics.iter().any(|t| prev.iter().any(|p| p == t || p.contains(t) || t.contains(p)));
        if overlap && !user_topics.is_empty() {
            metrics.repeat_topic_turns = metrics.repeat_topic_turns.saturating_add(1);
            metrics.clarification_rounds = metrics.clarification_rounds.saturating_add(1);
        }
    }

    if !user_topics.is_empty() {
        metrics.last_user_topics = Some(user_topics.to_vec());
    }
}

pub fn record_inject(metrics: &mut TopicMemoryMetrics, today: &str) {
    metrics.inject_count = metrics.inject_count.saturating_add(1);
    metrics.last_inject_at = Some(today.to_string());
}
