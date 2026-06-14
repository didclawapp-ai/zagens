//! Context compiler types — shared vocabulary for kernel-v2 Phase 2.
//!
//! **Phase 2-A scope:** type definitions + `ContextCompiler` skeleton; shadow
//! mode infrastructure wired in `runtime-server`. No production rendering
//! logic lives here yet — render closures are registered by runtime-server
//! callers and executed there.
//!
//! **Design:** [doc_Private/docs/tech/AGENT_KERNEL_V2_PHASE2_DESIGN.md]
//! **Acceptance gate:** `static_prefix_sha256` diff rate 0% in shadow mode.

use std::sync::Arc;

use crate::engine::token_estimate::TokenEstimator;
use crate::session::Session;
use crate::working_set::WorkingSet;

// ── Layer ─────────────────────────────────────────────────────────────────────

/// KV-cache layout layer for a `ContextSource`.
///
/// Determines where the source's rendered blocks land in the request prefix:
/// - `StaticPrefix` bytes are hashed into `static_prefix_sha256`.
/// - `SemiStatic` bytes are hashed into `full_prefix_sha256` (but not static).
/// - `Volatile` bytes change every step and are excluded from the static hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextLayer {
    /// System prompt static layer + tool catalog. Byte-stable across turns.
    StaticPrefix = 0,
    /// Session-fixed but turn-variable content (compaction summary, cycle briefing,
    /// topic memory). Stable within a cycle but may change at cycle boundaries.
    SemiStatic = 1,
    /// Per-step volatile content (turn_meta, scratchpad reminder, steer).
    Volatile = 2,
}

// ── Budget ────────────────────────────────────────────────────────────────────

/// Token budget policy for a `ContextSource`.
#[derive(Debug, Clone, Copy)]
pub enum BudgetPolicy {
    /// Hard-reserve exactly `n` tokens (system prompt static layer, tool catalog).
    Fixed(u32),
    /// Fraction of the total context window (0.0–1.0).
    Fraction(f32),
    /// Elastic allocation: guarantee `min`, allow up to `max` when budget permits.
    Elastic { min: u32, max: u32 },
}

// ── SourceId ──────────────────────────────────────────────────────────────────

/// Stable identifier for a `ContextSource`.
///
/// Used in fingerprint source-contribution maps and shadow diff logs.
/// Must be stable across restarts (do not derive from addresses or indices).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceId(pub &'static str);

impl std::fmt::Display for SourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

// ── RenderedBlock ─────────────────────────────────────────────────────────────

/// One rendered unit produced by a `ContextSource`.
///
/// A source may produce multiple blocks (e.g. a system prompt source produces
/// separate static and dynamic blocks). Blocks are concatenated in declaration
/// order within the same source.
#[derive(Debug, Clone)]
pub struct RenderedBlock {
    /// UTF-8 content to inject into the request.
    pub text: String,
    /// Pre-computed token estimate (from `estimate_text_tokens`).
    pub token_count: u32,
    /// Layer override; when `None`, inherits the parent `ContextSource::layer`.
    pub layer_override: Option<ContextLayer>,
}

impl RenderedBlock {
    /// Convenience constructor: estimate tokens from text automatically.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let token_count = crate::engine::token_estimate::estimate_text_tokens(&text) as u32;
        Self {
            text,
            token_count,
            layer_override: None,
        }
    }

    /// Constructor with explicit token count (e.g. when the caller already has
    /// a count from an API response).
    #[must_use]
    pub fn with_tokens(text: impl Into<String>, token_count: u32) -> Self {
        Self {
            text: text.into(),
            token_count,
            layer_override: None,
        }
    }
}

// ── SourceContribution ────────────────────────────────────────────────────────

/// Token contribution summary for one source — carried in `CompiledContext`.
#[derive(Debug, Clone)]
pub struct SourceContribution {
    pub source_id: SourceId,
    pub token_count: u32,
    pub was_truncated: bool,
}

// ── ContextProjection ─────────────────────────────────────────────────────────

/// Read-only aggregated view of core session state.
///
/// Passed to every `ContextSource::render` closure so that render functions
/// can be pure: they take this snapshot and produce `Vec<RenderedBlock>`.
///
/// **Phase 2 transition note:** In Phase 3, all fields are replaced by
/// deterministic EventLog projections and this type disappears.  Phase 2
/// render closures registered in runtime-server capture `&ContextProjection`
/// references; the boundary is explicit.
pub struct ContextProjection<'a> {
    /// Conversation history and session parameters.
    pub session: &'a Session,
    /// Repo-aware working set (for `<turn_meta>` generation).
    pub working_set: &'a WorkingSet,
    /// Current step index within the turn (0-based).
    pub step_idx: u32,
    /// Whether a compaction summary is present on the session.
    pub has_compaction_summary: bool,
    /// Number of cycle briefings in the session.
    pub cycle_briefing_count: usize,
}

impl<'a> ContextProjection<'a> {
    /// Build a projection from live session state.
    #[must_use]
    pub fn from_session(session: &'a Session, step_idx: u32) -> Self {
        Self {
            has_compaction_summary: session.compaction_summary_prompt.is_some(),
            cycle_briefing_count: session.cycle_briefings.len(),
            working_set: &session.working_set,
            session,
            step_idx,
        }
    }
}

// ── ContextSource ─────────────────────────────────────────────────────────────

/// Type alias for the render closure to keep `ContextSource` field types readable.
pub type RenderFn = Arc<dyn Fn(&ContextProjection<'_>) -> Vec<RenderedBlock> + Send + Sync>;

/// A registered context source: declaration + render closure.
///
/// Sources are registered by runtime-server (which has access to system-prompt
/// assembly code).  Core only defines the contract.
pub struct ContextSource {
    /// Stable identifier for diagnostics and diff reports.
    pub id: SourceId,
    /// KV-cache layer (determines fingerprint coverage).
    pub layer: ContextLayer,
    /// Sort key within the same layer: higher priority sources render first and
    /// are preserved last during budget overflow eviction.
    pub priority: u8,
    /// Token budget policy.
    pub budget: BudgetPolicy,
    /// Pure render function: given a projection, produce content blocks.
    ///
    /// Closures must be `Send + Sync` because the compiler may be shared
    /// across async tasks (e.g. sub-agent compilation on a thread pool).
    pub render: RenderFn,
}

impl std::fmt::Debug for ContextSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextSource")
            .field("id", &self.id)
            .field("layer", &self.layer)
            .field("priority", &self.priority)
            .finish_non_exhaustive()
    }
}

// ── CompiledContext ───────────────────────────────────────────────────────────

/// Output of `ContextCompiler::compile`.
///
/// Carries both the rendered content (for assembling into `MessageRequest`)
/// and observability metadata (token breakdown by source, fingerprint).
#[derive(Debug, Clone, Default)]
pub struct CompiledContext {
    /// All rendered text blocks, in the order they should appear in the request.
    /// System-layer blocks go into `MessageRequest::system`; message-layer blocks
    /// go into `messages_with_turn_metadata` (assembled by the caller).
    pub blocks: Vec<RenderedBlock>,
    /// Token contribution per source (for diagnostics and budget tuning).
    pub contributions: Vec<SourceContribution>,
    /// Total tokens across all blocks.
    pub total_tokens: u32,
    /// Whether any source was truncated due to budget constraints.
    pub any_truncated: bool,
}

// ── ContextCompilerMode ───────────────────────────────────────────────────────

/// Kill-switch mode for the context compiler (`[context] compiler` in config.toml).
///
/// Mirrors the `ToolsPolicyMode` / `ToolsSchedulerMode` pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextCompilerMode {
    /// Existing injection-point code runs unmodified (default).
    #[default]
    Legacy,
    /// ContextCompiler runs in parallel; fingerprint diffs are logged but the
    /// existing path still controls the request.  Gate: diff rate < 0.1%.
    Shadow,
    /// ContextCompiler controls the request; legacy injection code removed.
    V2,
}

impl ContextCompilerMode {
    /// Parse from an optional string value (e.g. from `config.toml`).
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("shadow") => Self::Shadow,
            Some("v2") => Self::V2,
            _ => Self::Legacy,
        }
    }

    /// Canonical string representation (round-trips with `parse`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Shadow => "shadow",
            Self::V2 => "v2",
        }
    }
}

// ── ContextCompiler ───────────────────────────────────────────────────────────

/// Registry of `ContextSource`s; entry point for Phase 2 compilation.
///
/// **P2-A scope:** `register()` + `render_all()` are the only methods.
/// `compile()` (budget solving + overflow recovery) is Phase 2-C work.
///
/// **P2-B:** Holds a [`TokenEstimator`] as the single calibration authority.
/// All block token counts produced by this compiler go through `TokenEstimator`,
/// ensuring the compiler budget accounting matches the capacity controller
/// and compaction trigger.
#[derive(Debug, Default)]
pub struct ContextCompiler {
    sources: Vec<ContextSource>,
    /// Canonical token estimator.  All `RenderedBlock` token counts produced
    /// by this compiler's `compile()` are verified against this estimator.
    pub token_estimator: TokenEstimator,
}

impl ContextCompiler {
    /// Create an empty compiler with default `TokenEstimator`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            token_estimator: TokenEstimator,
        }
    }

    /// Register a source.  Sources are sorted by (layer asc, priority desc)
    /// when `render_all` is called, not at registration time.
    #[must_use]
    pub fn register(mut self, source: ContextSource) -> Self {
        self.sources.push(source);
        self
    }

    /// Number of registered sources.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Call every registered render function and return (source_id, blocks) pairs,
    /// ordered by (layer asc, priority desc).  No budget enforcement yet (P2-C).
    #[must_use]
    pub fn render_all<'p>(
        &self,
        projection: &ContextProjection<'p>,
    ) -> Vec<(&SourceId, Vec<RenderedBlock>)> {
        let mut sorted: Vec<&ContextSource> = self.sources.iter().collect();
        sorted.sort_unstable_by(|a, b| a.layer.cmp(&b.layer).then(b.priority.cmp(&a.priority)));
        sorted
            .iter()
            .map(|src| (&src.id, (src.render)(projection)))
            .collect()
    }

    /// Flatten all rendered blocks into a single `CompiledContext`.
    ///
    /// No budget enforcement — all blocks are included.  P2-C will add
    /// `compile_with_budget_override` that enforces `BudgetPolicy` limits.
    ///
    /// **P2-B:** Token counts in `SourceContribution` are computed by
    /// `self.token_estimator.estimate_text()` rather than relying on the
    /// pre-populated `RenderedBlock.token_count`.  This ensures the compiler's
    /// budget accounting is always consistent with the capacity controller and
    /// compaction trigger (both also go through `TokenEstimator`).
    #[must_use]
    pub fn compile<'p>(&self, projection: &ContextProjection<'p>) -> CompiledContext {
        let rendered = self.render_all(projection);
        let mut out = CompiledContext::default();
        for (id, blocks) in rendered {
            let mut source_tokens: u32 = 0;
            for block in &blocks {
                // P2-B: use TokenEstimator for authoritative token counting.
                let canonical = self.token_estimator.estimate_text(&block.text) as u32;
                source_tokens = source_tokens.saturating_add(canonical);
            }
            out.total_tokens = out.total_tokens.saturating_add(source_tokens);
            out.contributions.push(SourceContribution {
                source_id: id.clone(),
                token_count: source_tokens,
                was_truncated: false,
            });
            out.blocks.extend(blocks);
        }
        out
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn dummy_source(id: &'static str, layer: ContextLayer, priority: u8) -> ContextSource {
        ContextSource {
            id: SourceId(id),
            layer,
            priority,
            budget: BudgetPolicy::Elastic { min: 0, max: 4096 },
            render: Arc::new(move |_| vec![RenderedBlock::new(format!("block:{id}"))]),
        }
    }

    fn test_session() -> crate::session::Session {
        crate::session::Session::new(
            "test-model".into(),
            PathBuf::from("/tmp"),
            false,
            false,
            PathBuf::from("/tmp/notes.txt"),
            PathBuf::from("/tmp/mcp.json"),
        )
    }

    #[test]
    fn compiler_render_order_is_layer_then_priority_desc() {
        let compiler = ContextCompiler::new()
            .register(dummy_source("volatile.low", ContextLayer::Volatile, 10))
            .register(dummy_source("static.high", ContextLayer::StaticPrefix, 255))
            .register(dummy_source("semi.mid", ContextLayer::SemiStatic, 128))
            .register(dummy_source("static.low", ContextLayer::StaticPrefix, 10));

        let session = test_session();
        let proj = ContextProjection::from_session(&session, 0);
        let rendered = compiler.render_all(&proj);
        let ids: Vec<&str> = rendered.iter().map(|(id, _)| id.0).collect();

        // Expected order: static.high(255) → static.low(10) → semi.mid → volatile.low
        assert_eq!(
            ids,
            ["static.high", "static.low", "semi.mid", "volatile.low"]
        );
    }

    #[test]
    fn compiled_context_aggregates_token_counts() {
        let compiler = ContextCompiler::new()
            .register(dummy_source("a", ContextLayer::StaticPrefix, 100))
            .register(dummy_source("b", ContextLayer::Volatile, 50));

        let session = test_session();
        let proj = ContextProjection::from_session(&session, 0);
        let ctx = compiler.compile(&proj);

        assert_eq!(ctx.contributions.len(), 2);
        assert_eq!(
            ctx.total_tokens,
            ctx.contributions.iter().map(|c| c.token_count).sum::<u32>()
        );
    }

    #[test]
    fn context_compiler_mode_parse_roundtrip() {
        for (input, expected) in [
            (Some("shadow"), ContextCompilerMode::Shadow),
            (Some("v2"), ContextCompilerMode::V2),
            (Some("legacy"), ContextCompilerMode::Legacy),
            (None, ContextCompilerMode::Legacy),
            (Some("SHADOW"), ContextCompilerMode::Shadow),
            (Some("unknown"), ContextCompilerMode::Legacy),
        ] {
            let mode = ContextCompilerMode::parse(input);
            assert_eq!(mode, expected, "input={input:?}");
            if expected != ContextCompilerMode::Legacy || input == Some("legacy") {
                assert_eq!(ContextCompilerMode::parse(Some(mode.as_str())), mode);
            }
        }
    }
}
