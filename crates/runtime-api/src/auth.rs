//! Runtime API bearer-token authentication middleware.

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::state::RuntimeApiAuthState;

fn constant_time_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.bytes()
        .zip(right.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

fn bearer_token_from_header(value: &str) -> Option<&str> {
    value.strip_prefix("Bearer ")
}

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
        .and_then(bearer_token_from_header)
        .is_some_and(|token| constant_time_eq(token, expected))
        || req
            .headers()
            .get("x-deepseek-runtime-token")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|token| constant_time_eq(token, expected));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_equal_strings() {
        assert!(constant_time_eq("abc", "abc"));
    }

    #[test]
    fn constant_time_eq_rejects_different_strings() {
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
    }
}
