//! Topic memory graph HTTP handlers (B-L3 read-only).

use axum::Json;
use axum::extract::State;
use deepseek_topic_memory::{
    PheromoneGraph, TopicMemoryEvalReport, eval_report, load_graph, load_metrics,
    metrics_path_for_graph,
};
use serde::Serialize;

use crate::topic_memory;

use super::{ApiError, RuntimeApiState};

#[derive(Serialize)]
pub(crate) struct TopicMemoryResponse {
    pub enabled: bool,
    pub graph_path: String,
    pub graph: PheromoneGraph,
    pub metrics: TopicMemoryEvalReport,
}

pub(crate) async fn get_topic_memory(
    State(state): State<RuntimeApiState>,
) -> Result<Json<TopicMemoryResponse>, ApiError> {
    let settings = topic_memory::settings_from_config(&state.config);
    let graph_path = settings.graph_path.clone();
    let graph = if settings.enabled {
        load_graph(&graph_path)
    } else {
        deepseek_topic_memory::empty_graph()
    };
    let metrics_raw = load_metrics(&metrics_path_for_graph(&graph_path));
    Ok(Json(TopicMemoryResponse {
        enabled: settings.enabled,
        graph_path: graph_path.display().to_string(),
        graph,
        metrics: eval_report(&metrics_raw),
    }))
}
