//! Runtime API bearer-token authentication middleware.
//!
//! When `RuntimeApiState.runtime_token` is `Some`, all `/v1/*` routes
//! are gated behind this middleware.  Requests must carry the token
//! in either the `Authorization: Bearer <token>` header or the
//! `x-deepseek-runtime-token` header.

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use super::RuntimeApiState;

pub(super) async fn require_runtime_token(
    State(state): State<RuntimeApiState>,
    req: Request,
    next: Next,
) -> Response {
    let Some(expected) = state.runtime_token.as_deref() else {
        return next.run(req).await;
    };
    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected)
        || req
            .headers()
            .get("x-deepseek-runtime-token")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|token| token == expected);

    if authorized {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": {
                    "message": "runtime API bearer token required",
                    "status": StatusCode::UNAUTHORIZED.as_u16(),
                }
            })),
        )
            .into_response()
    }
}
