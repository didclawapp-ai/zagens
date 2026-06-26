//! Runtime-wide status probes (active turns, etc.).

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use super::{ApiError, RuntimeApiState};

#[derive(Debug, Serialize)]
pub struct ActiveTurnEntry {
    pub thread_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct ActiveTurnsResponse {
    pub count: usize,
    pub threads: Vec<ActiveTurnEntry>,
}

pub(crate) async fn get_runtime_active_turns(
    State(state): State<RuntimeApiState>,
) -> Result<Json<ActiveTurnsResponse>, ApiError> {
    let list = state.runtime_threads.thread_status_list().await;
    let threads: Vec<ActiveTurnEntry> = list
        .into_iter()
        .filter(|(_, entry)| entry.status.is_active())
        .map(|(thread_id, entry)| ActiveTurnEntry {
            thread_id,
            turn_id: entry.turn_id,
            status: entry.status.as_str().to_string(),
        })
        .collect();
    let count = threads.len();
    Ok(Json(ActiveTurnsResponse { count, threads }))
}
