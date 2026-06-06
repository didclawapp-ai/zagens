//! Router composition — public routes and CORS (D16 E1-c phase 2).
//!
//! Bearer auth on `/v1/*` is applied by the sidecar before calling [`compose_router`]
//! so Axum state types stay aligned when merging route trees.

use axum::Router;
use axum::routing::get;

use crate::cors::cors_layer;
use crate::health::{health, internal_probe};
use crate::state::RuntimeApiHostState;

/// Mount `/health`, `/internal/probe`, merge authenticated `/v1/*`, and CORS defaults.
///
/// `api_routes` must already include the bearer-token [`route_layer`](axum::Router::route_layer)
/// from the host crate.
pub fn compose_router<S>(state: S, cors_origins: &[String], api_routes: Router<S>) -> Router
where
    S: RuntimeApiHostState,
{
    Router::new()
        .route("/health", get(health))
        .route("/internal/probe", get(internal_probe::<S>))
        .merge(api_routes)
        .layer(cors_layer(cors_origins))
        .with_state(state)
}
