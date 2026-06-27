//! Inbound channel events — external push into a running thread (P2).

use axum::Json;
use axum::extract::{Path as AxumPath, State};

use crate::runtime_threads::ChannelEventRequest;

use super::{ApiError, RuntimeApiState, map_thread_err};

pub(crate) async fn post_thread_channel_event(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<ChannelEventRequest>,
) -> Result<Json<crate::runtime_threads::ChannelEventResponse>, ApiError> {
    let response = state
        .runtime_threads
        .inject_channel_event(&id, req)
        .await
        .map_err(map_thread_err)?;
    Ok(Json(response))
}
