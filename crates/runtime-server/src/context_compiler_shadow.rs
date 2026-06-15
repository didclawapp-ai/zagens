//! Context-compiler V2 state snapshot and source-graph construction.
//!
//! Phase 2 P2-Switch complete (2026-06-15): `ContextCompiler` is the sole
//! request-assembly path.  Shadow bake passed (0-diff); shadow mode and legacy
//! injection code removed.
//!
//! This module provides:
//! - [`ContextCompilerStateSnapshot`] — pre-rendered strings from live engine state
//! - [`build_compiler_from_snapshot`] — assembles the 7-source `ContextCompiler`
//! - [`assemble_system_text_for_v2`] — combines StaticPrefix + SemiStatic layers
//! - [`scratchpad_reminder_est_tokens`] — pure-logic budget placeholder

use std::sync::Arc;

use zagens_core::engine::{
    BudgetPolicy, ContextCompiler, ContextLayer, ContextProjection, ContextSource, RenderedBlock,
    SourceId,
};
use zagens_core::session::Session;

// ── Engine state snapshot ─────────────────────────────────────────────────────

/// Pre-rendered strings extracted from live engine state.
///
/// All fields are `String` so the snapshot is `'static` and can be
/// moved into render closures without lifetime issues.  The values are
/// captured once at `model_request_fingerprint` time and represent exactly
/// what the legacy rendering path already produced.
///
/// **Source → field mapping:**
///
/// | ContextSource id       | Field                      | Notes                                            |
/// |------------------------|----------------------------|--------------------------------------------------|
/// | `system.static`        | `static_base_text`         | Block 0 text up to COMPACT_TEMPLATE              |
/// | `memory.compaction`    | `compaction_text`          | Block 0 after COMPACT_TEMPLATE + blocks 1+ joined|
/// | `memory.cycle`         | `cycle_briefings_text`     | Rendered cycle briefing text (→ messages)        |
/// | `working_set`          | `working_set_text`         | `<turn_meta>` block                              |
/// | `tools.catalog`        | `tool_catalog_est_tokens`  | Placeholder budget (actual JSON assembled later) |
/// | `scratchpad.reminder`  | — (rendered empty)         | Volatile; actual text injected by legacy path    |
/// | `steer`                | — (rendered empty)         | Volatile; arrives via channel, unknown at snapshot|
#[derive(Debug, Clone, Default)]
pub struct ContextCompilerStateSnapshot {
    /// System prompt text up to and including COMPACT_TEMPLATE (static portion of block 0).
    pub static_base_text: String,
    /// System prompt text after COMPACT_TEMPLATE in block 0 plus any compaction summary
    /// blocks (joined with `"\n\n---\n\n"` to match `system_to_instructions` output).
    pub compaction_text: String,
    /// Pre-rendered cycle briefing text (goes into messages at runtime, registered here for
    /// token-budget accounting in `compile_with_budget_override`).
    pub cycle_briefings_text: String,
    /// Working-set turn-meta block text (pre-rendered by existing path).
    pub working_set_text: String,
    /// Estimated token count for the tool catalog (StaticPrefix budget placeholder).
    ///
    /// Default: [`TOOL_CATALOG_BUDGET_TOKENS`].
    /// When active tools are available (e.g. via `context_compiler_system_prompt`'s
    /// `active_tools` param), the caller should override this with the serialized
    /// JSON token estimate for accurate budget accounting.
    pub tool_catalog_est_tokens: u32,
    /// Current step index within the turn (0-based).
    pub step_idx: u32,
    /// Estimated token count for the scratchpad reminder that *may* be injected
    /// at the end of the current step (pure-logic, no filesystem I/O).
    ///
    /// Non-zero when `scratchpad_step.readonly_tool_successes >=
    /// config.remind_after_readonly_tools` AND no scratchpad writes this step.
    /// Populated by `compiler_request_context` (L2) after calling
    /// [`scratchpad_reminder_est_tokens`].  Used for budget accounting only;
    /// actual reminder injection still happens via the legacy
    /// `maybe_inject_scratchpad_reminder` path.
    pub scratchpad_reminder_est_tokens: u32,
}

/// Approximate token footprint of a scratchpad reminder message.
///
/// The actual message is a short sentence with an area path; 80 tokens is a
/// conservative upper bound.  Used exclusively for budget-solver accounting;
/// the real message size is determined at injection time.
pub const SCRATCHPAD_REMINDER_TOKEN_ESTIMATE: u32 = 80;

/// Pure-logic predicate for whether a scratchpad reminder would be injected.
///
/// Returns [`SCRATCHPAD_REMINDER_TOKEN_ESTIMATE`] when the reminder threshold
/// is crossed, `0` otherwise.  No filesystem I/O — all inputs come from
/// in-memory step state and config.
#[must_use]
pub fn scratchpad_reminder_est_tokens(
    config: &zagens_core::scratchpad::ScratchpadConfig,
    step: &zagens_core::engine::scratchpad_state::ScratchpadStepState,
) -> u32 {
    if config.enabled
        && config.remind_enabled
        && step.scratchpad_writes_this_step == 0
        && step.readonly_tool_successes >= config.remind_after_readonly_tools
    {
        SCRATCHPAD_REMINDER_TOKEN_ESTIMATE
    } else {
        0
    }
}

/// Default tool-catalog token budget (StaticPrefix placeholder).
///
/// Approximates the token cost of the full built-in tool catalog (~50–80 tools,
/// ~200–250 tokens each).  Replaced with exact counts in the P2-message-path PR
/// when the active-tools list is threaded into the snapshot.
pub const TOOL_CATALOG_BUDGET_TOKENS: u32 = 12_000;

impl ContextCompilerStateSnapshot {
    /// Build a snapshot from live session state.
    #[must_use]
    pub fn from_session(session: &Session, step_idx: u32) -> Self {
        let tpl = crate::prompts::COMPACT_TEMPLATE;

        // Flatten the full system prompt text (same as `system_to_instructions`).
        let full_text: String = match session.system_prompt.as_ref() {
            None => String::new(),
            Some(crate::models::SystemPrompt::Text(t)) => t.clone(),
            Some(crate::models::SystemPrompt::Blocks(blocks)) => blocks
                .iter()
                .map(|b| b.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n---\n\n"),
        };

        // Split at COMPACT_TEMPLATE boundary.
        let (static_base_text, compaction_text) = if let Some(pos) = full_text.find(tpl) {
            let split = pos + tpl.len();
            (
                full_text[..split].to_string(),
                full_text[split..].to_string(),
            )
        } else {
            (full_text, String::new())
        };

        // Render cycle briefings for token accounting.
        let cycle_briefings_text = render_cycle_briefings(&session.cycle_briefings);

        let working_set_text = working_set_turn_meta(session, &session.workspace);

        Self {
            static_base_text,
            compaction_text,
            cycle_briefings_text,
            working_set_text,
            tool_catalog_est_tokens: TOOL_CATALOG_BUDGET_TOKENS,
            step_idx,
            // Not computable from session alone — populated by compiler_request_context (L2).
            scratchpad_reminder_est_tokens: 0,
        }
    }
}

// ── Source registration ───────────────────────────────────────────────────────

/// Build a `ContextCompiler` with all registered sources from a state snapshot.
///
/// **Full source map (post P2-missing-sources PR):**
///
/// | source id             | layer        | priority | budget                          |
/// |-----------------------|--------------|----------|---------------------------------|
/// | `system.static`       | StaticPrefix | 255      | Fixed(8192) — hard reserve      |
/// | `tools.catalog`       | StaticPrefix | 254      | Fixed(12000) — placeholder      |
/// | `memory.compaction`   | SemiStatic   | 200      | Elastic { min:0, max:4000 }     |
/// | `memory.cycle`        | SemiStatic   | 170      | Elastic { min:0, max:3000 }     |
/// | `working_set`         | Volatile     | 160      | Elastic { min:0, max:1500 }     |
/// | `scratchpad.reminder` | Volatile     | 140      | Elastic { min:0, max:800 }      |
/// | `steer`               | Volatile     | 100      | Elastic { min:0, max:2000 }     |
///
/// `tools.catalog` renders a budget placeholder — actual JSON bytes are assembled
/// in `streaming_phase` and threaded into the fingerprint separately.  Actual
/// `scratchpad.reminder` and `steer` renders are still injected by the legacy path;
/// these registrations provide budget-accounting entries only.
#[must_use]
pub fn build_compiler_from_snapshot(snapshot: &ContextCompilerStateSnapshot) -> ContextCompiler {
    let static_text = snapshot.static_base_text.clone();
    let compaction_text = snapshot.compaction_text.clone();
    let cycle_text = snapshot.cycle_briefings_text.clone();
    let working_set_text = snapshot.working_set_text.clone();
    let tool_catalog_tokens = snapshot.tool_catalog_est_tokens;
    let scratchpad_reminder_tokens = snapshot.scratchpad_reminder_est_tokens;

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
            id: SourceId("tools.catalog"),
            layer: ContextLayer::StaticPrefix,
            priority: 254,
            budget: BudgetPolicy::Fixed(tool_catalog_tokens),
            render: Arc::new(move |_| {
                // Actual catalog JSON is assembled in streaming_phase outside the compiler.
                // Return a placeholder block so the budget solver reserves the right amount.
                vec![RenderedBlock::placeholder(tool_catalog_tokens)]
            }),
        })
        .register(ContextSource {
            id: SourceId("memory.compaction"),
            layer: ContextLayer::SemiStatic,
            priority: 200,
            budget: BudgetPolicy::Elastic { min: 0, max: 4000 },
            render: Arc::new(move |_| {
                if compaction_text.is_empty() {
                    vec![]
                } else {
                    vec![RenderedBlock::new(compaction_text.clone())]
                }
            }),
        })
        .register(ContextSource {
            id: SourceId("memory.cycle"),
            layer: ContextLayer::SemiStatic,
            priority: 170,
            budget: BudgetPolicy::Elastic { min: 0, max: 3000 },
            render: Arc::new(move |_| {
                if cycle_text.is_empty() {
                    vec![]
                } else {
                    vec![RenderedBlock::new(cycle_text.clone())]
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
        .register(ContextSource {
            id: SourceId("scratchpad.reminder"),
            layer: ContextLayer::Volatile,
            priority: 140,
            budget: BudgetPolicy::Elastic { min: 0, max: 800 },
            // Budget-accounting placeholder derived from step state (no I/O).
            // Non-zero only when the reminder threshold is crossed; actual
            // text is still injected by the legacy `maybe_inject_scratchpad_reminder`
            // path (as a persistent session message).
            render: Arc::new(move |_| {
                if scratchpad_reminder_tokens > 0 {
                    vec![RenderedBlock::placeholder(scratchpad_reminder_tokens)]
                } else {
                    vec![]
                }
            }),
        })
        .register(ContextSource {
            id: SourceId("steer"),
            layer: ContextLayer::Volatile,
            priority: 100,
            budget: BudgetPolicy::Elastic { min: 0, max: 2000 },
            // Steer text arrives mid-turn via channel — unknown at snapshot time.
            render: Arc::new(|_| vec![]),
        })
}

/// Assemble the system prompt text for V2 mode from a state snapshot.
///
/// Combines `static_base_text` (StaticPrefix layer) and `compaction_text`
/// (SemiStatic layer) to reproduce the full system-prompt string.  Cycle
/// briefings and working-set turn-meta go into messages, not the system field.
///
/// In V2 mode, `streaming_phase` calls this instead of reading
/// `session.system_prompt` directly, delegating all system-text assembly to
/// the `ContextCompiler` source graph.
#[must_use]
pub fn assemble_system_text_for_v2(snapshot: &ContextCompilerStateSnapshot) -> String {
    format!("{}{}", snapshot.static_base_text, snapshot.compaction_text)
}

// ── Session helpers ───────────────────────────────────────────────────────────

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

/// Render cycle briefings as a single string for token-budget accounting.
///
/// Cycle briefings are injected as messages at runtime; this helper produces
/// the combined text so the budget solver can estimate their token footprint.
pub fn render_cycle_briefings(briefings: &[zagens_core::cycle::CycleBriefing]) -> String {
    briefings
        .iter()
        .filter(|b| !b.briefing_text.trim().is_empty())
        .map(|b| {
            format!(
                "[CYCLE BRIEFING — cycle {} at {}]\n{}",
                b.cycle,
                b.timestamp.to_rfc3339(),
                b.briefing_text.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_compiler_from_snapshot_registers_expected_sources() {
        let marker = crate::prompts::COMPACT_TEMPLATE;
        let snapshot = ContextCompilerStateSnapshot {
            static_base_text: format!("static base\n\n{marker}"),
            compaction_text: "after-marker".into(),
            cycle_briefings_text: String::new(),
            working_set_text: "Current local date: 2099-01-01".into(),
            tool_catalog_est_tokens: TOOL_CATALOG_BUDGET_TOKENS,
            scratchpad_reminder_est_tokens: 0,
            step_idx: 0,
        };
        let compiler = build_compiler_from_snapshot(&snapshot);
        assert_eq!(
            compiler.source_count(),
            7,
            "system.static + tools.catalog + memory.compaction + memory.cycle + working_set + scratchpad.reminder + steer"
        );
    }

    #[test]
    fn snapshot_static_text_matches_marker_boundary() {
        let marker = crate::prompts::COMPACT_TEMPLATE;
        let base = "base content";
        let extra = "compaction content";

        let snapshot = ContextCompilerStateSnapshot {
            static_base_text: format!("{base}\n\n{marker}"),
            compaction_text: extra.to_string(),
            cycle_briefings_text: String::new(),
            working_set_text: String::new(),
            tool_catalog_est_tokens: TOOL_CATALOG_BUDGET_TOKENS,
            scratchpad_reminder_est_tokens: 0,
            step_idx: 0,
        };
        let compiler = build_compiler_from_snapshot(&snapshot);
        let session = crate::core::session::Session::new(
            "test".to_string(),
            std::path::PathBuf::from("/tmp"),
            false,
            false,
            std::path::PathBuf::from("/tmp/notes.txt"),
            std::path::PathBuf::from("/tmp/mcp.json"),
        );
        let proj = ContextProjection::from_session(&session, 0);
        let ctx = compiler.compile(&proj);

        let static_src = ctx
            .contributions
            .iter()
            .find(|c| c.source_id.0 == "system.static")
            .expect("system.static source missing");
        let compaction_src = ctx
            .contributions
            .iter()
            .find(|c| c.source_id.0 == "memory.compaction");

        assert!(
            static_src.token_count > 0,
            "system.static must produce tokens"
        );
        if !extra.is_empty() {
            let comp_count = compaction_src.map(|c| c.token_count).unwrap_or(0);
            assert!(
                comp_count > 0,
                "memory.compaction must produce tokens for dynamic content"
            );
        }
    }

    #[test]
    fn render_cycle_briefings_empty_when_no_briefings() {
        let text = render_cycle_briefings(&[]);
        assert!(text.is_empty());
    }

    #[test]
    fn render_cycle_briefings_includes_cycle_number_and_text() {
        use chrono::Utc;
        use zagens_core::cycle::CycleBriefing;

        let briefings = vec![
            CycleBriefing {
                cycle: 1,
                timestamp: Utc::now(),
                briefing_text: "Decisions: chose A.".into(),
                token_estimate: 10,
            },
            CycleBriefing {
                cycle: 2,
                timestamp: Utc::now(),
                briefing_text: "Completed phase 1.".into(),
                token_estimate: 12,
            },
        ];
        let text = render_cycle_briefings(&briefings);
        assert!(text.contains("cycle 1"), "must reference cycle 1");
        assert!(text.contains("cycle 2"), "must reference cycle 2");
        assert!(text.contains("Decisions: chose A."));
        assert!(text.contains("Completed phase 1."));
    }

    #[test]
    fn snapshot_from_session_splits_at_compact_template() {
        use std::path::PathBuf;

        let marker = crate::prompts::COMPACT_TEMPLATE;
        let workspace = PathBuf::from("/tmp");
        let mut session = Session::new(
            "test-model".into(),
            workspace.clone(),
            false,
            false,
            PathBuf::from("/tmp/notes.txt"),
            PathBuf::from("/tmp/mcp.json"),
        );
        let full_text = format!("base text\n\n{marker}\nvolatile section");
        session.system_prompt = Some(crate::models::SystemPrompt::Text(full_text.clone()));

        let snapshot = ContextCompilerStateSnapshot::from_session(&session, 0);
        // static_base_text should contain the marker
        assert!(
            snapshot.static_base_text.contains(marker),
            "static_base_text must include COMPACT_TEMPLATE"
        );
        // compaction_text should be everything after the marker
        assert_eq!(
            snapshot.compaction_text, "\nvolatile section",
            "compaction_text must be text after COMPACT_TEMPLATE"
        );
        // Reassembling should reproduce the full text
        let reassembled = format!("{}{}", snapshot.static_base_text, snapshot.compaction_text);
        assert_eq!(reassembled, full_text);
    }
}
