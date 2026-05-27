//! Runtime API bearer-token authentication middleware.

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::state::RuntimeApiAuthState;

pub async fn require_runtime_token<S>(
    State(state): State<S>,
    req: Request,
    next: Next,
) -> Response
where
    S: RuntimeApiAuthState,
{
    let Some(expected) = state.runtime_token() else {
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
