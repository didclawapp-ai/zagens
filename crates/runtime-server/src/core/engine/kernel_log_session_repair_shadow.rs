//! Shadow counters for v3 log-driven session transcript repair (Phase 3b 5c).

use std::sync::atomic::{AtomicU64, Ordering};

static REPAIR_RUNS: AtomicU64 = AtomicU64::new(0);
static REPAIR_ROWS: AtomicU64 = AtomicU64::new(0);
static REPAIR_SKIPPED_ALIGNED: AtomicU64 = AtomicU64::new(0);
static REPAIR_PERSIST_OK: AtomicU64 = AtomicU64::new(0);
static REPAIR_PERSIST_FAILED: AtomicU64 = AtomicU64::new(0);
static REPAIR_PERSIST_SKIPPED_NO_SESSION: AtomicU64 = AtomicU64::new(0);

pub fn record_log_session_repair_run(repaired_rows: u64) {
    REPAIR_RUNS.fetch_add(1, Ordering::Relaxed);
    REPAIR_ROWS.fetch_add(repaired_rows, Ordering::Relaxed);
}

pub fn record_log_session_repair_skipped_aligned() {
    REPAIR_SKIPPED_ALIGNED.fetch_add(1, Ordering::Relaxed);
}

pub fn record_log_session_repair_persist_ok() {
    REPAIR_PERSIST_OK.fetch_add(1, Ordering::Relaxed);
}

pub fn record_log_session_repair_persist_failed() {
    REPAIR_PERSIST_FAILED.fetch_add(1, Ordering::Relaxed);
}

pub fn record_log_session_repair_persist_skipped_no_session() {
    REPAIR_PERSIST_SKIPPED_NO_SESSION.fetch_add(1, Ordering::Relaxed);
}

#[must_use]
pub fn kernel_log_session_repair_shadow_stats() -> (u64, u64, u64) {
    (
        REPAIR_RUNS.load(Ordering::Relaxed),
        REPAIR_ROWS.load(Ordering::Relaxed),
        REPAIR_SKIPPED_ALIGNED.load(Ordering::Relaxed),
    )
}

#[must_use]
pub fn kernel_log_session_repair_persist_shadow_stats() -> (u64, u64, u64) {
    (
        REPAIR_PERSIST_OK.load(Ordering::Relaxed),
        REPAIR_PERSIST_FAILED.load(Ordering::Relaxed),
        REPAIR_PERSIST_SKIPPED_NO_SESSION.load(Ordering::Relaxed),
    )
}
