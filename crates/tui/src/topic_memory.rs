//! Topic memory graph integration (B2) — opt-in via `[topic_memory]` in config.

use std::path::{Path, PathBuf};

use deepseek_topic_memory::{
    apply_decay, as_system_block, empty_graph, extract_topics, generate_memory_section,
    load_graph, load_metrics, metrics_path_for_graph, record_inject, record_turn_update,
    retrieve_for_query, save_graph, save_metrics, should_inject_memory, update_graph,
    GenerateMemorySectionOptions, TopicMemoryMetrics, DEFAULT_INJECT_INTERVAL_RUNS,
    DEFAULT_RETRIEVE_K_HOPS,
};

/// Resolved topic-memory settings (opt-in by default).
#[derive(Debug, Clone)]
pub struct TopicMemorySettings {
    pub enabled: bool,
    pub graph_path: PathBuf,
    pub inject_interval: u32,
    pub retrieve_k_hops: usize,
    pub attribution: Option<String>,
}

impl Default for TopicMemorySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            graph_path: default_graph_path(),
            inject_interval: DEFAULT_INJECT_INTERVAL_RUNS,
            retrieve_k_hops: DEFAULT_RETRIEVE_K_HOPS,
            attribution: None,
        }
    }
}

/// Default storage directory: `~/.deepseek/topic-memory/` (not beside `config.toml`).
#[must_use]
pub fn default_topic_memory_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".deepseek")
        .join("topic-memory")
}

#[must_use]
pub fn default_graph_path() -> PathBuf {
    default_topic_memory_dir().join("graph.json")
}

/// Legacy path before the dedicated `topic-memory/` directory (same parent as `config.toml`).
fn legacy_graph_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".deepseek")
        .join("topic-memory.json")
}

/// One-time move from `~/.deepseek/topic-memory.json` → `~/.deepseek/topic-memory/graph.json`.
fn migrate_legacy_graph_if_needed(target: &Path) {
    if target.exists() {
        return;
    }
    let legacy = legacy_graph_path();
    if !legacy.is_file() {
        return;
    }
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::rename(&legacy, target);
}

/// Build settings from workspace `Config` + optional project TOML table.
#[must_use]
pub fn settings_from_config(cfg: &crate::config::Config) -> TopicMemorySettings {
    let mut s = TopicMemorySettings::default();
    if let Some(tm) = cfg.topic_memory.as_ref() {
        if let Some(enabled) = tm.enabled {
            s.enabled = enabled;
        }
        if let Some(ref p) = tm.graph_path {
            s.graph_path = crate::config::expand_path(p);
        }
        if let Some(n) = tm.inject_interval {
            s.inject_interval = n.max(1);
        }
        s.attribution = tm.attribution.clone();
    }
    if std::env::var("DEEPSEEK_TOPIC_MEMORY")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes"))
        .unwrap_or(false)
    {
        s.enabled = true;
    }
    s
}

/// In-memory turn counter for inject cadence (per engine instance).
#[derive(Debug, Default)]
pub struct TopicMemoryRuntime {
    pub runs_since_last_inject: u32,
}

fn load_or_init_metrics(settings: &TopicMemorySettings) -> TopicMemoryMetrics {
    load_metrics(&metrics_path_for_graph(&settings.graph_path))
}

fn persist_metrics(settings: &TopicMemorySettings, metrics: &TopicMemoryMetrics) {
    let path = metrics_path_for_graph(&settings.graph_path);
    let _ = save_metrics(&path, metrics);
}

impl TopicMemoryRuntime {
    /// Record a completed turn; updates graph on disk when enabled.
    pub fn on_turn_complete(
        &mut self,
        settings: &TopicMemorySettings,
        user_text: &str,
        assistant_text: &str,
    ) {
        if !settings.enabled {
            return;
        }
        let path = &settings.graph_path;
        migrate_legacy_graph_if_needed(path);
        let mut graph = load_graph(path);
        graph = apply_decay(&graph);
        graph = update_graph(&graph, user_text, assistant_text);
        let _ = save_graph(path, &graph);
        self.runs_since_last_inject = self.runs_since_last_inject.saturating_add(1);

        let mut metrics = load_or_init_metrics(settings);
        let user_topics = extract_topics(user_text);
        record_turn_update(&mut metrics, &user_topics);
        persist_metrics(settings, &metrics);
    }

    /// Build `<topic_memory>` block when inject interval is met (B2.3 k-hop when `query_hint` set).
    #[must_use]
    pub fn compose_block(
        &mut self,
        settings: &TopicMemorySettings,
        query_hint: Option<&str>,
    ) -> Option<String> {
        if !settings.enabled {
            return None;
        }
        migrate_legacy_graph_if_needed(&settings.graph_path);
        let graph = load_graph(&settings.graph_path);
        if !should_inject_memory(
            &graph,
            self.runs_since_last_inject,
            settings.inject_interval,
        ) {
            return None;
        }

        let inject_graph = query_hint
            .filter(|q| !q.trim().is_empty())
            .map(|q| retrieve_for_query(&graph, q, settings.retrieve_k_hops))
            .unwrap_or(graph);

        let section = generate_memory_section(
            &inject_graph,
            settings.attribution.as_deref().map(|a| GenerateMemorySectionOptions {
                attribution: Some(a),
            }),
        );
        self.runs_since_last_inject = 0;

        let mut metrics = load_or_init_metrics(settings);
        record_inject(&mut metrics, &deepseek_topic_memory::today_str());
        persist_metrics(settings, &metrics);

        as_system_block(&section, &settings.graph_path)
    }
}

/// B2.1 — injection arbitration SSOT: [B2_INJECTION_ARBITRATION.md](docs/tech/adr/B2_INJECTION_ARBITRATION.md)
pub const INJECTION_ARBITRATION: &str =
    "tool results > CRAFT blackboard > topic_memory > user_memory > compaction summaries";

/// Last user message text and following assistant text (for graph update).
#[must_use]
pub fn last_exchange_from_messages(
    messages: &[crate::models::Message],
) -> Option<(String, String)> {
    use crate::models::ContentBlock;

    let mut last_user_idx = None;
    for (i, msg) in messages.iter().enumerate() {
        if msg.role == "user" {
            last_user_idx = Some(i);
        }
    }
    let user_idx = last_user_idx?;
    let user_text: String = messages[user_idx]
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    if user_text.trim().is_empty() {
        return None;
    }
    let assistant_text: String = messages
        .iter()
        .skip(user_idx + 1)
        .filter(|m| m.role == "assistant")
        .take(1)
        .flat_map(|m| &m.content)
        .filter_map(|b| match b {
            ContentBlock::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    if assistant_text.trim().is_empty() {
        return None;
    }
    Some((user_text, assistant_text))
}

/// Latest user message text for k-hop seeding.
#[must_use]
pub fn last_user_query_from_messages(messages: &[crate::models::Message]) -> Option<String> {
    last_exchange_from_messages(messages).map(|(u, _)| u)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn opt_in_default_off() {
        let cfg = crate::config::Config::default();
        let s = settings_from_config(&cfg);
        assert!(!s.enabled);
    }

    #[test]
    fn turn_updates_graph_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("g.json");
        let settings = TopicMemorySettings {
            enabled: true,
            graph_path: path.clone(),
            inject_interval: 1,
            retrieve_k_hops: 2,
            attribution: None,
        };
        let mut rt = TopicMemoryRuntime::default();
        for _ in 0..3 {
            rt.on_turn_complete(&settings, "讨论 Rust 性能", "可以用 profiling");
        }
        assert!(path.exists());
        let metrics_path = metrics_path_for_graph(&path);
        assert!(metrics_path.exists());
        rt.runs_since_last_inject = settings.inject_interval;
        let block = rt.compose_block(&settings, Some("Rust 性能优化"));
        assert!(block.is_some());
        assert!(block.unwrap().contains("topic_memory"));
    }
}
