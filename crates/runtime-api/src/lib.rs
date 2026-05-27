//! Runtime HTTP/SSE API types and OpenAPI export (D16 E1-c).
//!
//! Route handlers remain in `deepseek-runtime-server` until the full router migrates.

pub mod auth;
pub mod cors;
pub mod error;
pub mod health;
pub mod openapi;
pub mod router;
pub mod state;
pub mod task;

pub use auth::require_runtime_token;
pub use cors::cors_layer;
pub use error::ApiError;
pub use openapi::{
    ResumeSessionResponse, SessionDetailResponse, SessionsListResponse, StartTurnResponse,
    StreamTurnRequest, ThreadSummary,
};
pub use router::compose_router;
pub use state::{RuntimeApiAuthState, RuntimeApiHostState, RuntimeApiProbeState};
