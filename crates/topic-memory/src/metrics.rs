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
        let overlap = user_topics.iter().any(|t| {
            prev.iter()
                .any(|p| p == t || p.contains(t) || t.contains(p))
        });
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

/// B2.5 — derived rates for evaluation scripts and HTTP `/v1/topic-memory`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TopicMemoryEvalReport {
    pub turn_updates: u64,
    pub inject_count: u64,
    pub clarification_rounds: u64,
    pub repeat_topic_turns: u64,
    /// `clarification_rounds / turn_updates` (0 when no turns).
    pub clarification_rate: f64,
    /// `repeat_topic_turns / turn_updates`.
    pub repeat_topic_rate: f64,
    /// Proxy for long-session usefulness: injects per 10 turns.
    pub injects_per_10_turns: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_inject_at: Option<String>,
}

#[must_use]
pub fn eval_report(metrics: &TopicMemoryMetrics) -> TopicMemoryEvalReport {
    let turns = metrics.turn_updates.max(1) as f64;
    TopicMemoryEvalReport {
        turn_updates: metrics.turn_updates,
        inject_count: metrics.inject_count,
        clarification_rounds: metrics.clarification_rounds,
        repeat_topic_turns: metrics.repeat_topic_turns,
        clarification_rate: metrics.clarification_rounds as f64 / turns,
        repeat_topic_rate: metrics.repeat_topic_turns as f64 / turns,
        injects_per_10_turns: metrics.inject_count as f64 / turns * 10.0,
        last_inject_at: metrics.last_inject_at.clone(),
    }
}

/// Compare current metrics to a baseline file; used by `scripts/topic-memory-eval.ps1`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TopicMemoryEvalComparison {
    pub current: TopicMemoryEvalReport,
    pub baseline: TopicMemoryEvalReport,
    pub clarification_rate_delta: f64,
    pub repeat_topic_rate_delta: f64,
    /// True when clarification rate worsened by more than `max_regression` (absolute).
    pub regression: bool,
}

#[must_use]
pub fn compare_eval(
    current: &TopicMemoryMetrics,
    baseline: &TopicMemoryMetrics,
    max_regression: f64,
) -> TopicMemoryEvalComparison {
    let current = eval_report(current);
    let baseline = eval_report(baseline);
    let clarification_rate_delta = current.clarification_rate - baseline.clarification_rate;
    let repeat_topic_rate_delta = current.repeat_topic_rate - baseline.repeat_topic_rate;
    let regression = clarification_rate_delta > max_regression;
    TopicMemoryEvalComparison {
        current,
        baseline,
        clarification_rate_delta,
        repeat_topic_rate_delta,
        regression,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_report_rates() {
        let m = TopicMemoryMetrics {
            turn_updates: 10,
            inject_count: 2,
            clarification_rounds: 3,
            repeat_topic_turns: 3,
            ..Default::default()
        };
        let r = eval_report(&m);
        assert!((r.clarification_rate - 0.3).abs() < f64::EPSILON);
        assert!((r.injects_per_10_turns - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compare_detects_regression() {
        let baseline = TopicMemoryMetrics {
            turn_updates: 20,
            clarification_rounds: 2,
            ..Default::default()
        };
        let current = TopicMemoryMetrics {
            turn_updates: 20,
            clarification_rounds: 8,
            ..Default::default()
        };
        let cmp = compare_eval(&current, &baseline, 0.1);
        assert!(cmp.regression);
    }
}
