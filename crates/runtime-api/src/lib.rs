//! Runtime HTTP/SSE API types and OpenAPI export (D16 E1-c, D17 frozen).
//!
//! Route handlers remain in `zagens-cli` **by design** (D17
//! Architecture Freeze).  The engine→tools→route-handler closure forms an
//! internally co-located unit; further extraction would force artificial
//! trait hierarchies with a single implementation each.  This crate is the
//! stable boundary for OpenAPI schema export, auth/cors middleware, wire
//! response types, and router composition helpers.

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
