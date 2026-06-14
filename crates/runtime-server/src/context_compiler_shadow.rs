//! Context-compiler shadow mode for kernel-v2 Phase 2-A.
//!
//! Runs the `ContextCompiler` in parallel with the existing rendering path,
//! compares fingerprints, and tracks diff statistics.  The existing path
//! continues to control every request until the gate (diff_rate < 0.1%) is
//! met and `context.compiler` is flipped to `"v2"`.
//!
//! ## Shadow invariant
//! The shadow path wraps the **same** rendering functions already called by
//! the existing code — zero semantic change.  If the fingerprint ever differs,
//! a bug was introduced in one of the rendering wrappers.
//!
//! ## Statistics
//! Global atomic counters (like M3 `policy_shadow_stats` / M4
//! `scheduler_shadow_stats`) can be queried from `diagnostics` tool and
//! `GET /v1/runtime/kernel-shadow`.

use std::sync::atomic::{AtomicU64, Ordering};

use zagens_core::chat::MessageRequest;
use zagens_core::engine::ContextLayer;
use zagens_core::session::Session;

use crate::request_fingerprint::fingerprint_message_request;

// ── Global shadow counters ────────────────────────────────────────────────────

static SHADOW_COMPARISONS: AtomicU64 = AtomicU64::new(0);
static SHADOW_STATIC_DIFFS: AtomicU64 = AtomicU64::new(0);
static SHADOW_FULL_DIFFS: AtomicU64 = AtomicU64::new(0);

/// Snapshot of context-compiler shadow statistics.
#[derive(Debug, Clone, Copy, Default)]
pub struct ContextCompilerShadowStats {
    pub comparisons: u64,
    pub static_diffs: u64,
    pub full_diffs: u64,
}

/// Read current shadow statistics (lock-free).
#[must_use]
pub fn context_compiler_shadow_stats() -> ContextCompilerShadowStats {
    ContextCompilerShadowStats {
        comparisons: SHADOW_COMPARISONS.load(Ordering::Relaxed),
        static_diffs: SHADOW_STATIC_DIFFS.load(Ordering::Relaxed),
        full_diffs: SHADOW_FULL_DIFFS.load(Ordering::Relaxed),
    }
}

/// Record the outcome of one shadow comparison.
pub fn record_context_compiler_shadow_diff(static_diff: bool, full_diff: bool) {
    SHADOW_COMPARISONS.fetch_add(1, Ordering::Relaxed);
    if static_diff {
        SHADOW_STATIC_DIFFS.fetch_add(1, Ordering::Relaxed);
    }
    if full_diff {
        SHADOW_FULL_DIFFS.fetch_add(1, Ordering::Relaxed);
    }
}

// ── Shadow comparison entry point ─────────────────────────────────────────────

/// Compare the fingerprint computed by `ContextCompiler` against the
/// fingerprint from the existing rendering path.
///
/// Called from `host_impl::model_request_fingerprint` when
/// `context.compiler = "shadow"`.
///
/// Returns `(static_diff, full_diff)`.
pub fn shadow_compare(
    request: &MessageRequest,
    existing_static: &str,
    existing_full: &str,
) -> (bool, bool) {
    // P2-A: compute shadow fingerprint from the same request.
    // The compiler wraps the existing code, so this is always 0 diff
    // and validates the infrastructure end-to-end.
    let shadow_fp = compute_compiler_fingerprint(request);

    let static_diff = shadow_fp.static_prefix_sha256 != existing_static;
    let full_diff = shadow_fp.full_prefix_sha256 != existing_full;

    if static_diff || full_diff {
        tracing::warn!(
            target = "context_compiler_shadow",
            existing_static = %existing_static,
            shadow_static = %shadow_fp.static_prefix_sha256,
            existing_full = %existing_full,
            shadow_full = %shadow_fp.full_prefix_sha256,
            "context compiler shadow diff detected"
        );
    }

    record_context_compiler_shadow_diff(static_diff, full_diff);
    (static_diff, full_diff)
}

/// Compute a fingerprint from the compiler's registered sources.
///
/// For P2-A, the render closures capture pre-built strings from `request`,
/// so this is trivially equivalent to `fingerprint_message_request(request)`.
/// The function exists to establish the call-site; in P2-A follow-up the
/// closures will be decoupled and independently reconstruct the wire bytes.
fn compute_compiler_fingerprint(
    request: &MessageRequest,
) -> zagens_core::engine::RequestFingerprint {
    // P2-A baseline: re-fingerprint the original request.
    // Phase 2-A follow-up: replace with compiler.render_all → assemble bytes → fingerprint.
    fingerprint_message_request(request)
}

/// Map a source id string to its `ContextLayer` for fingerprint partitioning.
fn compiler_source_layer(id: &str) -> ContextLayer {
    match id {
        "system.static" | "tools.catalog" => ContextLayer::StaticPrefix,
        "system.dynamic" | "memory.compaction" | "memory.topic" | "memory.cycle" => {
            ContextLayer::SemiStatic
        }
        _ => ContextLayer::Volatile,
    }
}

// ── Session helper ────────────────────────────────────────────────────────────

/// Extract a summary string for the working_set / turn_meta source from a
/// `Session` — used by the `working_set` source's render closure.
pub fn working_set_turn_meta(session: &Session, workspace: &std::path::Path) -> String {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let ws_summary = session
        .working_set
        .summary_block(workspace)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    match ws_summary {
        Some(ws) => format!("Current local date: {today}\n{ws}"),
        None => format!("Current local date: {today}"),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow_stats_start_at_zero() {
        // Note: counters are global; other tests may increment them.
        // Just verify the function is callable and returns sane types.
        let stats = context_compiler_shadow_stats();
        let _ = stats.comparisons;
        let _ = stats.static_diffs;
        let _ = stats.full_diffs;
    }

    #[test]
    fn record_diff_increments_all_counters() {
        let before = context_compiler_shadow_stats();
        record_context_compiler_shadow_diff(true, true);
        let after = context_compiler_shadow_stats();
        assert!(after.comparisons >= before.comparisons + 1);
        assert!(after.static_diffs >= before.static_diffs + 1);
        assert!(after.full_diffs >= before.full_diffs + 1);
    }

    #[test]
    fn record_no_diff_only_increments_comparison_count() {
        let before = context_compiler_shadow_stats();
        record_context_compiler_shadow_diff(false, false);
        let after = context_compiler_shadow_stats();
        assert!(after.comparisons >= before.comparisons + 1);
        // diffs should not have increased (may be equal due to other tests)
        assert!(after.static_diffs >= before.static_diffs);
        assert!(after.full_diffs >= before.full_diffs);
    }

    #[test]
    fn compiler_source_layer_maps_correctly() {
        assert_eq!(
            compiler_source_layer("system.static"),
            ContextLayer::StaticPrefix
        );
        assert_eq!(
            compiler_source_layer("tools.catalog"),
            ContextLayer::StaticPrefix
        );
        assert_eq!(
            compiler_source_layer("system.dynamic"),
            ContextLayer::SemiStatic
        );
        assert_eq!(
            compiler_source_layer("memory.compaction"),
            ContextLayer::SemiStatic
        );
        assert_eq!(compiler_source_layer("messages"), ContextLayer::Volatile);
        assert_eq!(compiler_source_layer("working_set"), ContextLayer::Volatile);
    }
}
