//! Web browsing tool with multi-command support (search/open/click/find/screenshot).
//!
//! Mirrors the Codex harness `web.run` interface.

use std::time::Duration;

mod html;
mod page;
mod search;
mod state;
mod tool;
mod types;

pub use tool::WebRunTool;

pub(crate) const MAX_RESULTS: usize = 15;
pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 15_000;
pub(crate) const DEFAULT_OPEN_TIMEOUT_MS: u64 = 20_000;
pub(crate) const MAX_WEB_RUN_SESSIONS: usize = 64;
pub(crate) const MAX_PAGES_PER_SESSION: usize = 256;
pub(crate) const WEB_RUN_SESSION_TTL: Duration = Duration::from_secs(30 * 60);
pub(crate) const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

#[cfg(test)]
mod tests;
