//! Topic memory graph — incremental conversation topic map for prompt injection (B2).
//!
//! Port of [topic-memory-graph](https://github.com/didclawapp-ai/topic-memory-graph);
//! reference TS: `docs/topic-memory-graph-main/src/`.

mod engine;
mod extract;
mod graph;
mod inject;
mod metrics;
mod retrieve;
mod stopwords;

pub use engine::{
    DEFAULT_INJECT_INTERVAL_RUNS, GenerateMemorySectionOptions, apply_decay, empty_graph,
    generate_memory_section, should_inject_memory, today_str, update_graph,
};
pub use extract::{detect_blocked_topics, detect_emotion, extract_topics};
pub use graph::{
    BlockedPoint, CognitiveTrail, EmotionMode, GRAPH_SCHEMA_VERSION, PheromoneEdge, PheromoneGraph,
    PheromoneNode,
};
pub use inject::{DEFAULT_MARKERS, MemorySectionMarkers, inject_memory_section};
pub use metrics::{
    TopicMemoryEvalComparison, TopicMemoryEvalReport, TopicMemoryMetrics, compare_eval,
    eval_report, load_metrics, metrics_path_for_graph, record_inject, record_turn_update,
    save_metrics,
};
pub use retrieve::{
    DEFAULT_RETRIEVE_K_HOPS, induced_subgraph, retrieve_for_query, retrieve_k_hop_subgraph,
};

use std::fs;
use std::path::Path;

/// Load graph from JSON path; returns empty graph when missing or invalid.
#[must_use]
pub fn load_graph(path: &Path) -> PheromoneGraph {
    let Ok(raw) = fs::read_to_string(path) else {
        return empty_graph();
    };
    serde_json::from_str(&raw).unwrap_or_else(|_| empty_graph())
}

/// Persist graph atomically (best-effort).
pub fn save_graph(path: &Path, graph: &PheromoneGraph) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(graph)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(tmp, path)?;
    Ok(())
}

/// Wrap markdown in `<topic_memory>` for system prompt assembly.
#[must_use]
pub fn as_system_block(content: &str, source: &Path) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(format!(
        "<topic_memory source=\"{}\">\n{trimmed}\n</topic_memory>",
        source.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trip_save_load() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("topic-memory.json");
        let mut g = empty_graph();
        g = update_graph(&g, "hello Rust", "hi there");
        save_graph(&path, &g).expect("save");
        let loaded = load_graph(&path);
        assert_eq!(loaded.nodes.len(), g.nodes.len());
    }
}
