//! `GET /v1/threads/{id}/trace-report` — Kernel Trace Report export for desktop / tooling.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;

use super::{ApiError, RuntimeApiState, map_thread_err};
use crate::trace_export::{
    build_trace_bundle_for_thread, load_trace_report_template, render_trace_html,
};

#[derive(Debug, Deserialize)]
pub(crate) struct TraceReportQuery {
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

pub(crate) async fn get_thread_trace_report(
    State(state): State<RuntimeApiState>,
    Path(thread_id): Path<String>,
    Query(query): Query<TraceReportQuery>,
) -> Result<Response, ApiError> {
    let config = state.config.clone();
    let workspace = state.workspace.clone();
    let include_harness = query.include_harness;
    let format = query.format.clone();
    let redact = !query.no_redact;
    let thread_id = thread_id.clone();

    let bundle = tokio::task::spawn_blocking(move || {
        build_trace_bundle_for_thread(&thread_id, &config, &workspace, include_harness, redact)
    })
    .await
    .map_err(|e| ApiError::internal(format!("trace export task failed: {e}")))?
    .map_err(map_thread_err)?;

    match format.as_str() {
        "html" => {
            let html = tokio::task::spawn_blocking(move || {
                let template = load_trace_report_template(None).map_err(|e| e.to_string())?;
                render_trace_html(&bundle, &template).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| ApiError::internal(format!("trace html task failed: {e}")))?
            .map_err(ApiError::internal)?;

            Ok(Html(html).into_response())
        }
        "bundle" => {
            Ok(([(header::CONTENT_TYPE, "application/json")], Json(bundle)).into_response())
        }
        other => Err(ApiError::bad_request(format!(
            "unknown format {other:?} (expected html or bundle)"
        ))),
    }
}
