//! Panel poll intervals (align desktop `runtimePoll.ts`).

use std::time::Duration;

/// Checklist / harness poll while a turn is streaming.
pub const POLL_STREAMING_MS: u64 = 2000;

/// Idle poll interval.
pub const POLL_IDLE_MS: u64 = 5000;

#[must_use]
pub fn poll_interval(streaming: bool) -> Duration {
    Duration::from_millis(if streaming {
        POLL_STREAMING_MS
    } else {
        POLL_IDLE_MS
    })
}
