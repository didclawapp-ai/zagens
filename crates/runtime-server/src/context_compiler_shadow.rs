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
    BudgetPolicy, ContextAssemblyReport, ContextCompiler, ContextLayer, ContextProjection,
    ContextSource, RenderedBlock, SourceId,
};
use zagens_core::session::Session;

use crate::context_prompt_segments::{StaticPromptSegments, decompose_static_system_text};

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
    /// Static-prefix segments for Explorer rules/skills attribution (P2b+).
    pub static_segments: StaticPromptSegments,
    /// Builtin (non-MCP) tool catalog token estimate.
    pub tools_builtin_tokens: u32,
    /// MCP tool catalog token estimate.
    pub tools_mcp_tokens: u32,
    /// P3: `true` when the compaction summary already lives in the message
    /// transcript as a `[COMPACTED_HISTORY]` block (`summary_in_messages` mode).
    ///
    /// When set, [`assemble_system_text_for_v2`] must **not** re-append
    /// `compaction_text` to the wire `system` field — doing so would send the
    /// summary twice (once in `messages`, once in `system`). The text is still
    /// kept in `compaction_text` so the `memory.compaction` source remains
    /// visible to the budget solver and the Explorer `summarized` category.
    pub compaction_in_messages: bool,
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
        let (mut static_base_text, system_tail) = if let Some(pos) = full_text.find(tpl) {
            let split = pos + tpl.len();
            (
                full_text[..split].to_string(),
                full_text[split..].to_string(),
            )
        } else {
            (full_text, String::new())
        };

        // P3: full summary lives in messages; system tail is only a short pointer.
        let messages_compaction =
            crate::compaction::extract_compacted_history_text(&session.messages);
        let compaction_in_messages = messages_compaction.is_some();
        let compaction_text = messages_compaction
            .clone()
            .unwrap_or_else(|| system_tail.clone());
        if compaction_in_messages && !system_tail.is_empty() {
            static_base_text.push_str(&system_tail);
        }

        // Render cycle briefings for token accounting.
        let cycle_briefings_text = render_cycle_briefings(&session.cycle_briefings);

        let working_set_text = working_set_turn_meta(session, &session.workspace);
        let static_segments = decompose_static_system_text(&static_base_text);

        Self {
            static_base_text,
            compaction_text,
            cycle_briefings_text,
            working_set_text,
            tool_catalog_est_tokens: TOOL_CATALOG_BUDGET_TOKENS,
            step_idx,
            // Not computable from session alone — populated by compiler_request_context (L2).
            scratchpad_reminder_est_tokens: 0,
            static_segments,
            tools_builtin_tokens: TOOL_CATALOG_BUDGET_TOKENS,
            tools_mcp_tokens: 0,
            compaction_in_messages,
        }
    }
}

// ── Source registration ───────────────────────────────────────────────────────

/// Build a `ContextCompiler` with all registered sources from a state snapshot.
#[must_use]
pub fn build_compiler_from_snapshot(snapshot: &ContextCompilerStateSnapshot) -> ContextCompiler {
    let system_core = snapshot.static_segments.system_core.clone();
    let rules_text = snapshot.static_segments.rules.clone();
    let skills_text = snapshot.static_segments.skills.clone();
    let compaction_text = snapshot.compaction_text.clone();
    let cycle_text = snapshot.cycle_briefings_text.clone();
    let working_set_text = snapshot.working_set_text.clone();
    let tools_builtin_tokens = snapshot.tools_builtin_tokens;
    let tools_mcp_tokens = snapshot.tools_mcp_tokens;
    let scratchpad_reminder_tokens = snapshot.scratchpad_reminder_est_tokens;

    let mut compiler = ContextCompiler::new();

    if !system_core.is_empty() {
        compiler = compiler.register(ContextSource {
            id: SourceId("system.core"),
            layer: ContextLayer::StaticPrefix,
            priority: 255,
            budget: BudgetPolicy::Fixed(8192),
            render: Arc::new(move |_| vec![RenderedBlock::new(system_core.clone())]),
        });
    }

    if !rules_text.is_empty() {
        compiler = compiler.register(ContextSource {
            id: SourceId("rules.aggregate"),
            layer: ContextLayer::StaticPrefix,
            priority: 253,
            budget: BudgetPolicy::Elastic { min: 0, max: 8000 },
            render: Arc::new(move |_| vec![RenderedBlock::new(rules_text.clone())]),
        });
    }

    if !skills_text.is_empty() {
        compiler = compiler.register(ContextSource {
            id: SourceId("skills.catalog"),
            layer: ContextLayer::StaticPrefix,
            priority: 252,
            budget: BudgetPolicy::Elastic { min: 0, max: 4000 },
            render: Arc::new(move |_| vec![RenderedBlock::new(skills_text.clone())]),
        });
    }

    if tools_builtin_tokens > 0 {
        compiler = compiler.register(ContextSource {
            id: SourceId("tools.builtin"),
            layer: ContextLayer::StaticPrefix,
            priority: 251,
            budget: BudgetPolicy::Fixed(tools_builtin_tokens),
            render: Arc::new(move |_| vec![RenderedBlock::placeholder(tools_builtin_tokens)]),
        });
    }

    if tools_mcp_tokens > 0 {
        compiler = compiler.register(ContextSource {
            id: SourceId("tools.mcp"),
            layer: ContextLayer::StaticPrefix,
            priority: 250,
            budget: BudgetPolicy::Fixed(tools_mcp_tokens),
            render: Arc::new(move |_| vec![RenderedBlock::placeholder(tools_mcp_tokens)]),
        });
    }

    compiler
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
            render: Arc::new(move |_| vec![]),
        })
}

/// Build an assembly report from session state (store / offline breakdown).
#[must_use]
pub fn assembly_report_from_session(session: &Session, step_idx: u32) -> ContextAssemblyReport {
    let snapshot = ContextCompilerStateSnapshot::from_session(session, step_idx);
    let compiler = build_compiler_from_snapshot(&snapshot);
    let compiled = compiler.compile(&ContextProjection::from_session(session, step_idx));
    let message_tokens = zagens_core::engine::context_usage_breakdown::conversation_message_tokens(
        &session.messages,
    );
    ContextAssemblyReport::from_compiled(&compiled).with_message_tokens(message_tokens)
}

/// Assemble the system prompt text for V2 mode from a state snapshot.
///
/// Combines `static_base_text` (StaticPrefix layer) and `compaction_text`
/// (SemiStatic layer) to reproduce the full system-prompt string.  Cycle
/// briefings and working-set turn-meta go into messages, not the system field.
///
/// **P3:** when `compaction_in_messages` is set, the summary already lives in
/// the message transcript as a `[COMPACTED_HISTORY]` block, so it is **omitted**
/// here — appending it would send the summary twice (once in `messages`, once
/// in `system`), inflating tokens and re-introducing the system-layer
/// "Frankenstein" the messages-layer redesign set out to remove. Legacy
/// (`summary_in_messages = false`) sessions keep the summary in `system`.
///
/// In V2 mode, `streaming_phase` calls this instead of reading
/// `session.system_prompt` directly, delegating all system-text assembly to
/// the `ContextCompiler` source graph.
#[must_use]
pub fn assemble_system_text_for_v2(snapshot: &ContextCompilerStateSnapshot) -> String {
    if snapshot.compaction_in_messages {
        snapshot.static_base_text.clone()
    } else {
        format!("{}{}", snapshot.static_base_text, snapshot.compaction_text)
    }
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
    use zagens_core::engine::ContextProjection;

    #[test]
    fn build_compiler_from_snapshot_registers_core_sources() {
        let marker = crate::prompts::COMPACT_TEMPLATE;
        let snapshot = ContextCompilerStateSnapshot {
            static_base_text: format!("static base\n\n{marker}"),
            compaction_text: "after-marker".into(),
            cycle_briefings_text: String::new(),
            working_set_text: "Current local date: 2099-01-01".into(),
            tool_catalog_est_tokens: TOOL_CATALOG_BUDGET_TOKENS,
            scratchpad_reminder_est_tokens: 0,
            step_idx: 0,
            static_segments: decompose_static_system_text(&format!("static base\n\n{marker}")),
            tools_builtin_tokens: TOOL_CATALOG_BUDGET_TOKENS,
            tools_mcp_tokens: 0,
            compaction_in_messages: false,
        };
        let compiler = build_compiler_from_snapshot(&snapshot);
        assert!(
            compiler.source_count() >= 5,
            "system.core + tools + memory + working_set + scratchpad + steer"
        );
    }

    #[test]
    fn build_compiler_registers_rules_and_skills_when_markers_present() {
        let marker = crate::prompts::COMPACT_TEMPLATE;
        let static_text = format!(
            "mode\n\n<project_instructions source=\"AGENTS.md\">\nrules\n</project_instructions>\n\n\
             ## Skills\n\n- demo: skill\n\n{marker}"
        );
        let snapshot = ContextCompilerStateSnapshot {
            static_base_text: static_text.clone(),
            compaction_text: String::new(),
            cycle_briefings_text: String::new(),
            working_set_text: String::new(),
            tool_catalog_est_tokens: TOOL_CATALOG_BUDGET_TOKENS,
            scratchpad_reminder_est_tokens: 0,
            step_idx: 0,
            static_segments: decompose_static_system_text(&static_text),
            tools_builtin_tokens: TOOL_CATALOG_BUDGET_TOKENS,
            tools_mcp_tokens: 0,
            compaction_in_messages: false,
        };
        let compiler = build_compiler_from_snapshot(&snapshot);
        let session = Session::new(
            "test".into(),
            std::path::PathBuf::from("/tmp"),
            false,
            false,
            std::path::PathBuf::from("/tmp/notes.txt"),
            std::path::PathBuf::from("/tmp/mcp.json"),
        );
        let compiled = compiler.compile(&ContextProjection::from_session(&session, 0));
        let categories: Vec<_> = compiled
            .assembly_report
            .spans
            .iter()
            .map(|s| s.category.as_str())
            .collect();
        assert!(categories.contains(&"rules"));
        assert!(categories.contains(&"skills"));
        assert!(categories.contains(&"system"));
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
            static_segments: decompose_static_system_text(&format!("{base}\n\n{marker}")),
            tools_builtin_tokens: TOOL_CATALOG_BUDGET_TOKENS,
            tools_mcp_tokens: 0,
            compaction_in_messages: false,
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
            .find(|c| c.source_id.0 == "system.core")
            .expect("system.core source missing");
        let compaction_src = ctx
            .contributions
            .iter()
            .find(|c| c.source_id.0 == "memory.compaction");

        assert!(
            static_src.token_count > 0,
            "system.core must produce tokens"
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

    #[test]
    fn snapshot_prefers_compacted_history_message_for_memory_compaction() {
        use std::path::PathBuf;
        use zagens_core::engine::COMPACTED_HISTORY_MARKER;

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
        let pointer = format!("\n{COMPACTED_HISTORY_MARKER}: archived summary in transcript.");
        session.system_prompt = Some(crate::models::SystemPrompt::Text(format!(
            "base text\n\n{marker}{pointer}"
        )));
        session.messages.push(crate::models::Message {
            role: "user".into(),
            content: vec![crate::models::ContentBlock::Text {
                text: format!("{COMPACTED_HISTORY_MARKER}\n\n<summary>full archive</summary>"),
                cache_control: None,
            }],
        });

        let snapshot = ContextCompilerStateSnapshot::from_session(&session, 0);
        assert!(
            snapshot.compaction_text.contains("full archive"),
            "memory.compaction source must read messages-layer summary"
        );
        assert!(
            snapshot.static_base_text.contains(pointer.trim()),
            "system pointer must remain in static prefix"
        );
        assert!(
            snapshot.compaction_in_messages,
            "compaction_in_messages must be set when a [COMPACTED_HISTORY] block exists"
        );
    }

    /// P3 dedup: when the summary already lives in `messages` as a
    /// `[COMPACTED_HISTORY]` block, the V2 wire `system` text must NOT re-append
    /// the full summary (otherwise the model receives it twice).
    #[test]
    fn assemble_system_text_omits_compaction_when_in_messages() {
        use std::path::PathBuf;
        use zagens_core::engine::COMPACTED_HISTORY_MARKER;

        let marker = crate::prompts::COMPACT_TEMPLATE;
        let mut session = Session::new(
            "test-model".into(),
            PathBuf::from("/tmp"),
            false,
            false,
            PathBuf::from("/tmp/notes.txt"),
            PathBuf::from("/tmp/mcp.json"),
        );
        let pointer =
            format!("\n{COMPACTED_HISTORY_MARKER}: archived summary lives in the transcript.");
        session.system_prompt = Some(crate::models::SystemPrompt::Text(format!(
            "base text\n\n{marker}{pointer}"
        )));
        let unique_summary = "UNIQUE_ARCHIVE_BODY_42";
        session.messages.push(crate::models::Message {
            role: "user".into(),
            content: vec![crate::models::ContentBlock::Text {
                text: format!("{COMPACTED_HISTORY_MARKER}\n\n<summary>{unique_summary}</summary>"),
                cache_control: None,
            }],
        });

        let snapshot = ContextCompilerStateSnapshot::from_session(&session, 0);
        let system_text = assemble_system_text_for_v2(&snapshot);

        // The pointer stays; the full archive body must appear exactly zero
        // times in the system field (it is sent via the message transcript).
        assert!(
            system_text.contains(COMPACTED_HISTORY_MARKER),
            "system pointer must survive"
        );
        assert!(
            !system_text.contains(unique_summary),
            "summary body must NOT be duplicated into the system field, got: {system_text}"
        );
        // But the snapshot still carries it for budget / Explorer accounting.
        assert!(
            snapshot.compaction_text.contains(unique_summary),
            "compaction_text must still hold the summary for budget/Explorer use"
        );
    }

    /// Legacy (`summary_in_messages = false`): summary lives in the system tail
    /// after COMPACT_TEMPLATE and must remain in the assembled system text.
    #[test]
    fn assemble_system_text_keeps_compaction_in_legacy_mode() {
        use std::path::PathBuf;

        let marker = crate::prompts::COMPACT_TEMPLATE;
        let mut session = Session::new(
            "test-model".into(),
            PathBuf::from("/tmp"),
            false,
            false,
            PathBuf::from("/tmp/notes.txt"),
            PathBuf::from("/tmp/mcp.json"),
        );
        let legacy_summary = "LEGACY_SYSTEM_SUMMARY_99";
        session.system_prompt = Some(crate::models::SystemPrompt::Text(format!(
            "base text\n\n{marker}\n{legacy_summary}"
        )));

        let snapshot = ContextCompilerStateSnapshot::from_session(&session, 0);
        assert!(
            !snapshot.compaction_in_messages,
            "no [COMPACTED_HISTORY] message → legacy mode"
        );
        let system_text = assemble_system_text_for_v2(&snapshot);
        assert!(
            system_text.contains(legacy_summary),
            "legacy summary must remain in the system field"
        );
    }

    #[test]
    fn compiler_snapshot_produces_conserved_assembly_report() {
        use std::path::PathBuf;
        use zagens_core::context_profile::{ContextThresholdOverrides, scaled_thresholds};
        use zagens_core::engine::ContextAssemblyReport;

        let workspace = PathBuf::from("/tmp");
        let mut session = Session::new(
            "deepseek-v4-pro".into(),
            workspace.clone(),
            false,
            false,
            PathBuf::from("/tmp/notes.txt"),
            PathBuf::from("/tmp/mcp.json"),
        );
        session.system_prompt = Some(crate::models::SystemPrompt::Text("static prompt".into()));
        session.messages.push(crate::models::Message {
            role: "user".to_string(),
            content: vec![crate::models::ContentBlock::Text {
                text: "hello".into(),
                cache_control: None,
            }],
        });

        let snapshot = ContextCompilerStateSnapshot::from_session(&session, 0);
        let compiler = build_compiler_from_snapshot(&snapshot);
        let compiled = compiler.compile(&ContextProjection::from_session(&session, 0));
        let message_tokens =
            zagens_core::engine::context_usage_breakdown::conversation_message_tokens(
                &session.messages,
            );
        let report =
            ContextAssemblyReport::from_compiled(&compiled).with_message_tokens(message_tokens);
        let thresholds = scaled_thresholds("deepseek-v4-pro", ContextThresholdOverrides::default());
        let breakdown = zagens_core::engine::build_context_usage_breakdown(
            "deepseek-v4-pro",
            Some(&report),
            report.estimated_input_tokens,
            1_000_000,
            &thresholds,
            true,
            false,
            session.messages.len(),
            Some(&session.messages),
        );
        assert_eq!(
            breakdown.category_token_sum(),
            breakdown.estimated_input_tokens
        );
    }
}
