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

    /// Budget-accounting placeholder: no renderable text, but reserves
    /// `token_count` tokens in the budget solver.
    ///
    /// Used for sources whose actual bytes are assembled outside the compiler
    /// (e.g. `tools.catalog`). The budget solver uses `token_count` directly
    /// when `text` is empty.
    #[must_use]
    pub fn placeholder(token_count: u32) -> Self {
        Self {
            text: String::new(),
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
    /// True when `compile_with_budget_override` had to drop or shrink sources
    /// to fit within the requested budget (P2-D).
    pub overflow_recovered: bool,
}

// ── P2-D: Budget override types ───────────────────────────────────────────────

/// Per-source budget override for `compile_with_budget_override`.
///
/// Allows the caller to lower (never raise) an individual source's budget
/// before the compiler's overflow solver runs.  `new_budget` replaces the
/// source's registered `BudgetPolicy` for this compilation only; the
/// registered policy is not mutated.
#[derive(Debug, Clone)]
pub struct BudgetOverride {
    pub source_id: SourceId,
    pub new_budget: BudgetPolicy,
}

/// Error returned by `compile_with_budget_override` when the budget cannot
/// be satisfied even after exhausting all eviction strategies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    /// Total rendered tokens exceeded `budget` after all overflow strategies
    /// (Volatile eviction + Elastic minimisation) were exhausted.
    Overflow {
        /// Token total after eviction (still over budget).
        total_tokens: u32,
        /// The requested budget limit.
        budget: u32,
    },
    /// No sources are registered; cannot produce any context.
    EmptySources,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overflow {
                total_tokens,
                budget,
            } => write!(
                f,
                "context overflow: {total_tokens} tokens > {budget} token budget after eviction"
            ),
            Self::EmptySources => write!(f, "no context sources registered"),
        }
    }
}

impl std::error::Error for CompileError {}

/// Kill-switch mode for the context compiler (`[context] compiler` in config.toml).
///
/// Mirrors the `ToolsPolicyMode` / `ToolsSchedulerMode` pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextCompilerMode {
    /// Kill-switch: existing injection-point code runs unmodified.
    /// Set `[context] compiler = "legacy"` in config.toml to restore pre-Phase-2 behaviour.
    Legacy,
    /// ContextCompiler runs in parallel; fingerprint diffs are logged but the
    /// existing path still controls the request.
    /// Set `[context] compiler = "shadow"` to re-enter the observation period.
    Shadow,
    /// ContextCompiler controls the request (default).
    /// Diff rate held at 0% in bake; legacy injection code no longer called for
    /// system-prompt assembly.
    #[default]
    V2,
}

impl ContextCompilerMode {
    /// Parse from an optional string value (e.g. from `config.toml`).
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("legacy") => Self::Legacy,
            Some("shadow") => Self::Shadow,
            Some("v2") => Self::V2,
            // Unknown / missing values → V2 (same as code default).
            _ => Self::V2,
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

    /// Compile with no budget enforcement (all blocks included).
    ///
    /// **P2-B:** Token counts in `SourceContribution` are computed by
    /// `self.token_estimator.estimate_text()` rather than relying on the
    /// pre-populated `RenderedBlock.token_count`.  This ensures the compiler's
    /// budget accounting is always consistent with the capacity controller and
    /// compaction trigger (both also go through `TokenEstimator`).
    #[must_use]
    pub fn compile<'p>(&self, projection: &ContextProjection<'p>) -> CompiledContext {
        let mut sorted: Vec<&ContextSource> = self.sources.iter().collect();
        sorted.sort_unstable_by(|a, b| a.layer.cmp(&b.layer).then(b.priority.cmp(&a.priority)));
        let blocks_per_source: Vec<Vec<RenderedBlock>> =
            sorted.iter().map(|s| (s.render)(projection)).collect();
        let source_tokens: Vec<u32> = blocks_per_source
            .iter()
            .map(|blocks| {
                blocks
                    .iter()
                    .map(|b| {
                        if b.text.is_empty() {
                            b.token_count
                        } else {
                            self.token_estimator.estimate_text(&b.text) as u32
                        }
                    })
                    .sum()
            })
            .collect();
        self.assemble_compiled(&sorted, &blocks_per_source, &source_tokens, &[], false)
    }

    /// Compile with an explicit token budget, applying progressive eviction if
    /// the unconstrained render would overflow.
    ///
    /// **Eviction strategy (in order):**
    /// 1. Volatile sources are excluded, lowest priority first, until under
    ///    budget.
    /// 2. SemiStatic Elastic sources are shrunk to their `min` allocation,
    ///    lowest priority first.
    /// 3. If still over budget after all above → `Err(CompileError::Overflow)`.
    ///
    /// Fixed and Fraction sources are never evicted; they represent hard
    /// requirements (system prompt, tool catalog).
    ///
    /// `overrides` temporarily replaces individual source budgets for this
    /// compilation only; they do not mutate registered source policies.
    ///
    /// Returns `CompiledContext::overflow_recovered = true` when at least one
    /// source was evicted or shrunk.
    pub fn compile_with_budget_override<'p>(
        &self,
        projection: &ContextProjection<'p>,
        budget: u32,
        overrides: &[BudgetOverride],
    ) -> Result<CompiledContext, CompileError> {
        if self.sources.is_empty() {
            return Err(CompileError::EmptySources);
        }

        // Build priority-sorted view of sources.
        let mut sorted: Vec<&ContextSource> = self.sources.iter().collect();
        sorted.sort_unstable_by(|a, b| a.layer.cmp(&b.layer).then(b.priority.cmp(&a.priority)));

        // Render all sources.
        let blocks_per_source: Vec<Vec<RenderedBlock>> =
            sorted.iter().map(|s| (s.render)(projection)).collect();

        // Compute canonical token counts.
        // Placeholder blocks (empty text, non-zero token_count) use their stored count;
        // all other blocks are estimated from text content.
        let source_tokens: Vec<u32> = blocks_per_source
            .iter()
            .map(|blocks| {
                blocks
                    .iter()
                    .map(|b| {
                        if b.text.is_empty() {
                            b.token_count
                        } else {
                            self.token_estimator.estimate_text(&b.text) as u32
                        }
                    })
                    .sum()
            })
            .collect();

        let total: u32 = source_tokens.iter().sum();
        if total <= budget {
            return Ok(self.assemble_compiled(
                &sorted,
                &blocks_per_source,
                &source_tokens,
                &[true; 0], // all enabled
                false,
            ));
        }

        // Enabled / effective-budget state (per source slot).
        let mut enabled: Vec<bool> = vec![true; sorted.len()];
        // Effective token budget after shrinking Elastic sources.
        let mut effective_tokens: Vec<u32> = source_tokens.clone();

        // Phase 1: evict Volatile sources, lowest priority first.
        // `sorted` is (layer asc, priority desc) → Volatile sources are at
        // the tail; walk backwards to drop lowest-priority ones first.
        let mut remaining: u32 = total;
        let mut overflow_recovered = false;

        for i in (0..sorted.len()).rev() {
            if remaining <= budget {
                break;
            }
            if sorted[i].layer == ContextLayer::Volatile && enabled[i] {
                remaining = remaining.saturating_sub(effective_tokens[i]);
                enabled[i] = false;
                overflow_recovered = true;
            }
        }

        // Phase 2: shrink SemiStatic Elastic sources to their min, lowest
        // priority first.
        for i in (0..sorted.len()).rev() {
            if remaining <= budget {
                break;
            }
            if sorted[i].layer == ContextLayer::SemiStatic && enabled[i] {
                let budget_policy = overrides
                    .iter()
                    .find(|o| o.source_id == sorted[i].id)
                    .map(|o| o.new_budget)
                    .unwrap_or(sorted[i].budget);
                if let BudgetPolicy::Elastic { min, .. } = budget_policy {
                    if effective_tokens[i] > min {
                        let freed = effective_tokens[i].saturating_sub(min);
                        remaining = remaining.saturating_sub(freed);
                        effective_tokens[i] = min;
                        overflow_recovered = true;
                    }
                }
            }
        }

        if remaining > budget {
            return Err(CompileError::Overflow {
                total_tokens: remaining,
                budget,
            });
        }

        Ok(self.assemble_compiled(
            &sorted,
            &blocks_per_source,
            &effective_tokens,
            &enabled,
            overflow_recovered,
        ))
    }

    /// Assemble a `CompiledContext` from pre-rendered per-source data.
    fn assemble_compiled(
        &self,
        sorted: &[&ContextSource],
        blocks_per_source: &[Vec<RenderedBlock>],
        source_tokens: &[u32],
        enabled: &[bool],
        overflow_recovered: bool,
    ) -> CompiledContext {
        let mut out = CompiledContext {
            overflow_recovered,
            ..Default::default()
        };
        let all_enabled = enabled.is_empty() || enabled.len() != sorted.len();
        for (idx, src) in sorted.iter().enumerate() {
            let is_enabled = all_enabled || enabled[idx];
            if !is_enabled {
                continue;
            }
            let tok = source_tokens[idx];
            out.total_tokens = out.total_tokens.saturating_add(tok);
            out.contributions.push(SourceContribution {
                source_id: src.id.clone(),
                token_count: tok,
                was_truncated: false,
            });
            out.blocks.extend_from_slice(&blocks_per_source[idx]);
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
        // Default (None / unknown) → V2 since P2-Switch (2026-06-14).
        // Legacy and Shadow are retained as explicit config values.
        for (input, expected) in [
            (Some("shadow"), ContextCompilerMode::Shadow),
            (Some("v2"), ContextCompilerMode::V2),
            (Some("legacy"), ContextCompilerMode::Legacy),
            (None, ContextCompilerMode::V2),
            (Some("SHADOW"), ContextCompilerMode::Shadow),
            (Some("unknown"), ContextCompilerMode::V2),
        ] {
            let mode = ContextCompilerMode::parse(input);
            assert_eq!(mode, expected, "input={input:?}");
            // Round-trip: parse(mode.as_str()) == mode for all explicit values.
            assert_eq!(ContextCompilerMode::parse(Some(mode.as_str())), mode);
        }
    }

    // ── P2-D budget-solver tests ──────────────────────────────────────────────

    fn budget_source(
        id: &'static str,
        layer: ContextLayer,
        priority: u8,
        text: &'static str,
        budget: BudgetPolicy,
    ) -> ContextSource {
        ContextSource {
            id: SourceId(id),
            layer,
            priority,
            budget,
            render: Arc::new(move |_| vec![RenderedBlock::new(text)]),
        }
    }

    /// When total tokens ≤ budget, `compile_with_budget_override` behaves like
    /// `compile` and sets `overflow_recovered = false`.
    #[test]
    fn budget_solve_no_eviction_when_under_budget() {
        let compiler = ContextCompiler::new()
            .register(budget_source(
                "static",
                ContextLayer::StaticPrefix,
                255,
                "system prompt text",
                BudgetPolicy::Fixed(200),
            ))
            .register(budget_source(
                "volatile",
                ContextLayer::Volatile,
                100,
                "turn meta",
                BudgetPolicy::Elastic { min: 0, max: 500 },
            ));

        let session = test_session();
        let proj = ContextProjection::from_session(&session, 0);

        // Very large budget — no eviction needed.
        let result = compiler.compile_with_budget_override(&proj, 100_000, &[]);
        assert!(result.is_ok(), "should succeed with huge budget");
        let ctx = result.unwrap();
        assert!(!ctx.overflow_recovered, "no eviction expected");
        assert_eq!(ctx.contributions.len(), 2);
    }

    /// Volatile sources are evicted first, lowest priority first.
    #[test]
    fn budget_solve_evicts_volatile_before_semistatic() {
        // Static: ~6 tokens (fixed, non-evictable)
        // SemiStatic: ~8 tokens (elastic min=0)
        // Volatile hi-pri: ~6 tokens
        // Volatile lo-pri: ~6 tokens
        let compiler = ContextCompiler::new()
            .register(budget_source(
                "static",
                ContextLayer::StaticPrefix,
                255,
                "system",
                BudgetPolicy::Fixed(100),
            ))
            .register(budget_source(
                "semi",
                ContextLayer::SemiStatic,
                180,
                "compaction summary",
                BudgetPolicy::Elastic { min: 0, max: 500 },
            ))
            .register(budget_source(
                "volatile.hi",
                ContextLayer::Volatile,
                160,
                "turn meta",
                BudgetPolicy::Elastic { min: 0, max: 500 },
            ))
            .register(budget_source(
                "volatile.lo",
                ContextLayer::Volatile,
                100,
                "steer text",
                BudgetPolicy::Elastic { min: 0, max: 500 },
            ));

        let session = test_session();
        let proj = ContextProjection::from_session(&session, 0);

        let unconstrained = compiler.compile(&proj);
        let total = unconstrained.total_tokens;

        // Budget that forces eviction of at least one volatile source.
        // We need to exclude at least one volatile to fit.
        let lo_tokens = unconstrained
            .contributions
            .iter()
            .find(|c| c.source_id.0 == "volatile.lo")
            .map(|c| c.token_count)
            .unwrap_or(0);

        // Set budget to total - lo_tokens - 1 (force eviction of at least lo).
        let budget = total.saturating_sub(lo_tokens).saturating_sub(1);
        let result = compiler.compile_with_budget_override(&proj, budget, &[]);
        assert!(result.is_ok(), "should succeed by evicting volatile.lo");
        let ctx = result.unwrap();
        assert!(
            ctx.overflow_recovered,
            "eviction should set overflow_recovered"
        );
        // volatile.lo should be excluded.
        assert!(
            ctx.contributions
                .iter()
                .all(|c| c.source_id.0 != "volatile.lo"),
            "volatile.lo should be evicted"
        );
        // static and semi should still be present.
        assert!(ctx.contributions.iter().any(|c| c.source_id.0 == "static"));
        assert!(ctx.contributions.iter().any(|c| c.source_id.0 == "semi"));
    }

    /// Returns `CompileError::Overflow` when even evicting all Volatile and
    /// shrinking all SemiStatic Elastic sources to their min is not enough.
    #[test]
    fn budget_solve_returns_overflow_when_fixed_sources_exceed_budget() {
        // Fixed source uses 4+ tokens. Budget of 1 is impossible.
        let compiler = ContextCompiler::new().register(budget_source(
            "static",
            ContextLayer::StaticPrefix,
            255,
            "this is a fixed system prompt that cannot be evicted",
            BudgetPolicy::Fixed(10_000),
        ));

        let session = test_session();
        let proj = ContextProjection::from_session(&session, 0);

        let result = compiler.compile_with_budget_override(&proj, 1, &[]);
        assert!(matches!(result, Err(CompileError::Overflow { .. })));
    }

    /// `CompileError::EmptySources` when no sources are registered.
    #[test]
    fn budget_solve_empty_compiler_returns_error() {
        let compiler = ContextCompiler::new();
        let session = test_session();
        let proj = ContextProjection::from_session(&session, 0);
        let result = compiler.compile_with_budget_override(&proj, 1000, &[]);
        assert!(
            matches!(result, Err(CompileError::EmptySources)),
            "empty compiler should return EmptySources"
        );
    }

    /// **P2-D step-latency gate.**
    ///
    /// `compile_with_budget_override` with a tight budget (triggering eviction)
    /// must complete in < 10 ms for a realistic compiler with 8 sources.
    /// This guards against O(n²) re-rendering bugs.
    #[test]
    fn budget_solve_latency_under_10ms() {
        use std::time::Instant;

        let mut compiler = ContextCompiler::new();
        for i in 0..8u8 {
            let layer = match i % 3 {
                0 => ContextLayer::StaticPrefix,
                1 => ContextLayer::SemiStatic,
                _ => ContextLayer::Volatile,
            };
            compiler = compiler.register(budget_source(
                // Safety: leak is fine in tests; static string only.
                Box::leak(format!("source-{i}").into_boxed_str()),
                layer,
                255u8.saturating_sub(i * 10),
                "representative content block with typical size payload",
                BudgetPolicy::Elastic { min: 10, max: 4096 },
            ));
        }

        let session = test_session();
        let proj = ContextProjection::from_session(&session, 0);

        let budget = 1; // force maximum eviction effort
        let start = Instant::now();
        let _ = compiler.compile_with_budget_override(&proj, budget, &[]);
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 10,
            "compile_with_budget_override took {}ms (must be < 10ms)",
            elapsed.as_millis()
        );
    }

    #[test]
    fn placeholder_block_token_count_used_by_budget_solver() {
        // Budget placeholder (empty text, non-zero token_count) should be
        // counted by the budget solver, not estimated as 0 from empty text.
        let compiler = ContextCompiler::new()
            .register(ContextSource {
                id: SourceId("tools.catalog"),
                layer: ContextLayer::StaticPrefix,
                priority: 254,
                budget: BudgetPolicy::Fixed(500),
                render: Arc::new(|_| vec![RenderedBlock::placeholder(500)]),
            })
            .register(ContextSource {
                id: SourceId("volatile.low"),
                layer: ContextLayer::Volatile,
                priority: 10,
                budget: BudgetPolicy::Elastic { min: 0, max: 200 },
                render: Arc::new(|_| {
                    vec![RenderedBlock::new("x".repeat(200 * 3))] // ~200 tokens
                }),
            });

        let session = test_session();
        let proj = ContextProjection::from_session(&session, 0);
        let ctx = compiler.compile(&proj);

        let catalog = ctx
            .contributions
            .iter()
            .find(|c| c.source_id.0 == "tools.catalog");
        assert_eq!(
            catalog.map(|c| c.token_count).unwrap_or(0),
            500,
            "placeholder token_count must be 500, not 0"
        );

        // Budget solver: total = 500 + ~200; if budget = 600, volatile should be evicted.
        // (budget=400 would fail because Fixed(500) > 400; Fixed sources cannot be evicted.)
        let result = compiler.compile_with_budget_override(&proj, 600, &[]);
        assert!(
            result.is_ok(),
            "budget solve should succeed: evict volatile to fit under 600"
        );
        let ctx2 = result.unwrap();
        let has_volatile = ctx2
            .contributions
            .iter()
            .any(|c| c.source_id.0 == "volatile.low");
        assert!(
            !has_volatile,
            "volatile.low should be evicted when total > 600"
        );
        let has_catalog = ctx2
            .contributions
            .iter()
            .any(|c| c.source_id.0 == "tools.catalog");
        assert!(has_catalog, "tools.catalog (Fixed) must survive eviction");
    }
}
