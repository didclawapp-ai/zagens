//! Test-only mirror of the production `llm_client` module surface.
//!
//! Re-exports the production [`LlmClient`] trait from `deepseek-core` so
//! `mock.rs` (included via `#[path]`) satisfies the same `#[async_trait]`
//! signatures as the binary.

pub use deepseek_core::chat::{LlmClient, StreamEventBox};

#[path = "../../src/llm_client/mock.rs"]
pub mod mock;
