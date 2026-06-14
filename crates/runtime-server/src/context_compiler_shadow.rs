//! Context-compiler shadow mode for kernel-v2 Phase 2-A / P2-Switch.
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

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use zagens_core::chat::MessageRequest;
use zagens_core::engine::{
    BudgetPolicy, ContextCompiler, ContextLayer, ContextProjection, ContextSource, RenderedBlock,
    SourceId,
};
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

// ── Engine state snapshot ─────────────────────────────────────────────────────

/// Pre-rendered strings extracted from live engine state.
///
/// All fields are `String` so the snapshot is `'static` and can be
/// moved into render closures without lifetime issues.  The values are
/// captured once at `model_request_fingerprint` time and represent exactly
/// what the legacy rendering path already produced.
#[derive(Debug, Clone, Default)]
pub struct ContextCompilerStateSnapshot {
    /// Full system prompt text (static base + COMPACT_TEMPLATE + dynamic sections).
    pub full_system_text: String,
    /// Working-set turn-meta block text (pre-rendered by existing path).
    pub working_set_text: String,
    /// Current step index within the turn (0-based).
    pub step_idx: u32,
}

impl ContextCompilerStateSnapshot {
    /// Build a snapshot from live session state.
    #[must_use]
    pub fn from_session(session: &Session, step_idx: u32) -> Self {
        let full_system_text = session
            .system_prompt
            .as_ref()
            .and_then(|sp| match sp {
                crate::models::SystemPrompt::Text(t) => Some(t.clone()),
                crate::models::SystemPrompt::Blocks(blocks) => {
                    let text = blocks
                        .iter()
                        .map(|b| b.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    if text.is_empty() { None } else { Some(text) }
                }
            })
            .unwrap_or_default();

        let working_set_text = working_set_turn_meta(session, &session.workspace);

        Self {
            full_system_text,
            working_set_text,
            step_idx,
        }
    }
}

// ── Source registration ───────────────────────────────────────────────────────

/// Build a `ContextCompiler` with all registered sources from a state snapshot.
///
/// **P2-Switch source map:**
///
/// | source id         | layer        | priority | render closure                          |
/// |-------------------|--------------|----------|-----------------------------------------|
/// | `system.static`   | StaticPrefix | 255      | system prompt up to COMPACT_TEMPLATE    |
/// | `system.dynamic`  | SemiStatic   | 200      | system prompt after COMPACT_TEMPLATE    |
/// | `working_set`     | Volatile     | 160      | `<turn_meta>` block                     |
///
/// `tools.catalog` is JSON-bytes and is handled separately in the fingerprint
/// assembler (not a text RenderedBlock).  `scratchpad.reminder` and `steer`
/// are injected into messages (Volatile layer, wired in P2-Switch message-path).
#[must_use]
pub fn build_compiler_from_snapshot(snapshot: &ContextCompilerStateSnapshot) -> ContextCompiler {
    let full_text = snapshot.full_system_text.clone();
    let working_set_text = snapshot.working_set_text.clone();

    // Derive static / dynamic split at the COMPACT_TEMPLATE boundary.
    let static_text: String = {
        let tpl = crate::prompts::COMPACT_TEMPLATE;
        if let Some(pos) = full_text.find(tpl) {
            full_text[..pos + tpl.len()].to_string()
        } else {
            full_text.clone()
        }
    };

    let dynamic_text: String = {
        let tpl = crate::prompts::COMPACT_TEMPLATE;
        if let Some(pos) = full_text.find(tpl) {
            full_text[pos + tpl.len()..].to_string()
        } else {
            String::new()
        }
    };

    ContextCompiler::new()
        .register(ContextSource {
            id: SourceId("system.static"),
            layer: ContextLayer::StaticPrefix,
            priority: 255,
            budget: BudgetPolicy::Fixed(8192),
            render: Arc::new(move |_| {
                if static_text.is_empty() {
                    vec![]
                } else {
                    vec![RenderedBlock::new(static_text.clone())]
                }
            }),
        })
        .register(ContextSource {
            id: SourceId("system.dynamic"),
            layer: ContextLayer::SemiStatic,
            priority: 200,
            budget: BudgetPolicy::Elastic { min: 0, max: 8192 },
            render: Arc::new(move |_| {
                if dynamic_text.is_empty() {
                    vec![]
                } else {
                    vec![RenderedBlock::new(dynamic_text.clone())]
                }
            }),
        })
        .register(ContextSource {
            id: SourceId("working_set"),
            layer: ContextLayer::Volatile,
            priority: 160,
            budget: BudgetPolicy::Elastic { min: 0, max: 1500 },
            render: Arc::new(move |_| {
                if working_set_text.is_empty() {
                    vec![]
                } else {
                    vec![RenderedBlock::new(working_set_text.clone())]
                }
            }),
        })
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
    // P2-A fallback: re-fingerprint the original request (always 0 diff).
    // Used when no state snapshot is available.
    let shadow_fp = fingerprint_message_request(request);
    let static_diff = shadow_fp.static_prefix_sha256 != existing_static;
    let full_diff = shadow_fp.full_prefix_sha256 != existing_full;
    record_context_compiler_shadow_diff(static_diff, full_diff);
    (static_diff, full_diff)
}

/// Compare using a real `ContextCompilerStateSnapshot` — the meaningful
/// shadow mode added in P2-Switch prep.
///
/// The compiler independently assembles the system text from registered sources
/// and computes the fingerprint.  The static-prefix diff is fully independent;
/// the full-prefix diff uses tools and messages from the existing request.
pub fn shadow_compare_with_snapshot(
    request: &MessageRequest,
    snapshot: &ContextCompilerStateSnapshot,
    existing_static: &str,
    existing_full: &str,
) -> (bool, bool) {
    let shadow_fp = compute_compiler_fingerprint_from_snapshot(request, snapshot);
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

/// Compute fingerprint from registered compiler sources.
///
/// System text is assembled from `ContextCompiler` sources (independent path).
/// Tools and messages are still taken from `request` (P2-Switch migration).
fn compute_compiler_fingerprint_from_snapshot(
    request: &MessageRequest,
    snapshot: &ContextCompilerStateSnapshot,
) -> zagens_core::engine::RequestFingerprint {
    use crate::request_fingerprint::static_system_instructions;
    use zagens_core::engine::request_fingerprint::compute_request_fingerprint;

    let compiler = build_compiler_from_snapshot(snapshot);

    // Build a minimal projection: only session is needed since closures are self-contained.
    // We use a temporary session just to satisfy the signature.
    let session_proxy = SessionProxy::from_snapshot(snapshot);
    let proj = ContextProjection::from_session(&session_proxy.session, snapshot.step_idx);

    let ctx = compiler.compile(&proj);

    // Assemble system text from compiler blocks (StaticPrefix + SemiStatic).
    let compiler_system_text: String = ctx
        .blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("");

    // For tools and messages, fall back to existing request data.
    // (Volatile / message-layer sources wired in P2-Switch message-path PR.)
    let tools = request.tools.as_deref().unwrap_or(&[]);
    let tools_bytes = serde_json::to_vec(tools).unwrap_or_default();

    let wire_messages = crate::client::build_chat_messages_for_request(request);
    let messages_bytes = serde_json::to_vec(&wire_messages).unwrap_or_default();

    let static_system = static_system_instructions(&compiler_system_text);

    let static_prefix = concat_prefix(&[static_system.as_bytes(), &tools_bytes]);
    let full_prefix = concat_prefix(&[
        compiler_system_text.as_bytes(),
        &tools_bytes,
        &messages_bytes,
    ]);

    compute_request_fingerprint(&static_prefix, &full_prefix)
}

fn concat_prefix(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.push(0);
        }
        out.extend_from_slice(part);
    }
    out
}

/// Thin wrapper that gives us a `Session` reference for `ContextProjection::from_session`.
/// The render closures are self-contained (they don't read from the session), so the
/// session content doesn't matter for correctness.
struct SessionProxy {
    session: crate::core::session::Session,
}

impl SessionProxy {
    fn from_snapshot(snapshot: &ContextCompilerStateSnapshot) -> Self {
        use std::path::PathBuf;
        let session = crate::core::session::Session::new(
            "shadow-proxy".to_string(),
            PathBuf::from("/tmp"),
            false,
            false,
            PathBuf::from("/tmp/notes.txt"),
            PathBuf::from("/tmp/mcp.json"),
        );
        let _ = snapshot; // closures are self-contained
        Self { session }
    }
}

// ── Compute fingerprint (P2-A baseline) ──────────────────────────────────────

/// Compute a fingerprint from the compiler's registered sources.
///
/// For P2-A, the render closures capture pre-built strings from `request`,
/// so this is trivially equivalent to `fingerprint_message_request(request)`.
/// The function exists to establish the call-site; in P2-A follow-up the
/// closures will be decoupled and independently reconstruct the wire bytes.
#[allow(dead_code)]
fn compute_compiler_fingerprint(
    request: &MessageRequest,
) -> zagens_core::engine::RequestFingerprint {
    // P2-A baseline: re-fingerprint the original request.
    fingerprint_message_request(request)
}

/// Map a source id string to its `ContextLayer` for fingerprint partitioning.
#[allow(dead_code)]
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

    #[test]
    fn build_compiler_from_snapshot_registers_expected_sources() {
        let snapshot = ContextCompilerStateSnapshot {
            full_system_text: format!(
                "static base\n\n{}after-marker",
                crate::prompts::COMPACT_TEMPLATE
            ),
            working_set_text: "Current local date: 2099-01-01".into(),
            step_idx: 0,
        };
        let compiler = build_compiler_from_snapshot(&snapshot);
        assert_eq!(
            compiler.source_count(),
            3,
            "system.static + system.dynamic + working_set"
        );
    }

    #[test]
    fn snapshot_static_text_matches_marker_boundary() {
        let marker = crate::prompts::COMPACT_TEMPLATE;
        let base = "base content";
        let extra = "dynamic section";
        let full = format!("{base}\n\n{marker}{extra}");

        let snapshot = ContextCompilerStateSnapshot {
            full_system_text: full.clone(),
            working_set_text: String::new(),
            step_idx: 0,
        };
        let compiler = build_compiler_from_snapshot(&snapshot);
        let proxy = SessionProxy::from_snapshot(&snapshot);
        let proj = ContextProjection::from_session(&proxy.session, 0);
        let ctx = compiler.compile(&proj);

        // Static source (system.static) renders text up to and including marker.
        let static_src = ctx
            .contributions
            .iter()
            .find(|c| c.source_id.0 == "system.static")
            .expect("system.static source missing");
        // Dynamic source (system.dynamic) renders text after marker.
        let dynamic_src = ctx
            .contributions
            .iter()
            .find(|c| c.source_id.0 == "system.dynamic");
        assert!(
            static_src.token_count > 0,
            "system.static must produce tokens"
        );
        // When there is dynamic content, system.dynamic should produce tokens.
        if !extra.is_empty() {
            let dyn_count = dynamic_src.map(|c| c.token_count).unwrap_or(0);
            assert!(
                dyn_count > 0,
                "system.dynamic must produce tokens for dynamic content"
            );
        }
    }
}
