//! `GET /v1/trace/compare` — side-by-side trace compare for desktop / tooling.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;

use super::{ApiError, RuntimeApiState, map_thread_err};
use crate::trace_export::{build_trace_compare_for_threads, load_trace_report_template};
use zagens_core::engine::embed_trace_compare_in_html;

#[derive(Debug, Deserialize)]
pub(crate) struct TraceCompareQuery {
    left: String,
    right: String,
    #[serde(default = "default_true")]
    include_harness: bool,
    #[serde(default = "default_html")]
    format: String,
    #[serde(default)]
    no_redact: bool,
}

fn default_true() -> bool {
    true
}

fn default_html() -> String {
    "html".to_string()
}

pub(crate) async fn get_trace_compare(
    State(state): State<RuntimeApiState>,
    Query(query): Query<TraceCompareQuery>,
) -> Result<Response, ApiError> {
    let left = query.left.trim().to_string();
    let right = query.right.trim().to_string();
    if left.is_empty() || right.is_empty() {
        return Err(ApiError::bad_request("left and right thread ids required"));
    }

    let config = state.config.clone();
    let workspace = state.workspace.clone();
    let include_harness = query.include_harness;
    let format = query.format.clone();
    let redact = !query.no_redact;

    let doc = tokio::task::spawn_blocking(move || {
        build_trace_compare_for_threads(&left, &right, &config, &workspace, include_harness, redact)
    })
    .await
    .map_err(|e| ApiError::internal(format!("trace compare task failed: {e}")))?
    .map_err(map_thread_err)?;

    match format.as_str() {
        "html" => {
            let html = tokio::task::spawn_blocking(move || {
                let template = load_trace_report_template(None).map_err(|e| e.to_string())?;
                embed_trace_compare_in_html(&template, &doc).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| ApiError::internal(format!("trace compare html task failed: {e}")))?
            .map_err(ApiError::internal)?;

            Ok(Html(html).into_response())
        }
        "bundle" => Ok(([(header::CONTENT_TYPE, "application/json")], Json(doc)).into_response()),
        other => Err(ApiError::bad_request(format!(
            "unknown format {other:?} (expected html or bundle)"
        ))),
    }
}
