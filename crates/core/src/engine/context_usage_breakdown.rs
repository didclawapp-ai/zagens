//! Context Explorer wire shape — categories + `next_action` (P2b).

use serde::{Deserialize, Serialize};

use crate::context_profile::{
    ContextProfile, ScaledContextThresholds, cycle_trigger_floor, resolve_context_profile,
};
use crate::engine::context::turn_response_headroom_tokens;

use super::context_assembly::ContextAssemblyReport;

/// Predicted transition action from profile + current fill (§3.5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextNextAction {
    None,
    SeamL1,
    SeamL2,
    SeamL3,
    Cycle,
    CompactSuggested,
    Overflow,
}

/// One Explorer category bucket (aggregated compiler spans).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextCategory {
    pub id: String,
    pub label: String,
    pub tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<ContextCategory>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_action_hint: Option<String>,
}

/// Full Explorer breakdown for HTTP / SSE (sibling to `ThreadContextSnapshot`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextUsageBreakdown {
    pub model: String,
    pub context_window_tokens: u32,
    pub estimated_input_tokens: u32,
    pub usage_percent: f64,
    pub profile: String,
    pub next_action: ContextNextAction,
    pub categories: Vec<ContextCategory>,
}

impl ContextUsageBreakdown {
    /// Sum of category token counts (conservation check vs `estimated_input_tokens`).
    #[must_use]
    pub fn category_token_sum(&self) -> u32 {
        self.categories.iter().map(|c| c.tokens).sum()
    }
}

/// Resolve the next transition action from profile thresholds (§3.5.1).
#[must_use]
pub fn resolve_next_action(
    model: &str,
    estimated_input: u64,
    thresholds: &ScaledContextThresholds,
    seam_enabled: bool,
    should_compact: bool,
) -> ContextNextAction {
    let headroom = turn_response_headroom_tokens();
    if let Some(window) = thresholds.window {
        if estimated_input >= u64::from(window) {
            return ContextNextAction::Overflow;
        }
    }

    let cycle_trigger = match thresholds.profile {
        ContextProfile::Large => cycle_trigger_floor(model, thresholds.cycle, headroom),
        _ => thresholds.cycle as u64,
    };
    if cycle_trigger > 0 && estimated_input >= cycle_trigger {
        return ContextNextAction::Cycle;
    }

    if thresholds.profile == ContextProfile::Large && seam_enabled {
        if estimated_input >= thresholds.l3 as u64 {
            return ContextNextAction::SeamL3;
        }
        if estimated_input >= thresholds.l2 as u64 {
            return ContextNextAction::SeamL2;
        }
        if estimated_input >= thresholds.l1 as u64 {
            return ContextNextAction::SeamL1;
        }
        return ContextNextAction::None;
    }

    if should_compact {
        return ContextNextAction::CompactSuggested;
    }

    ContextNextAction::None
}

#[must_use]
fn category_label(id: &str) -> String {
    match id {
        "system" => "System",
        "tools" => "Tools",
        "rules" => "Rules",
        "skills" => "Skills",
        "mcp" => "MCP",
        "subagents" => "Subagents",
        "conversation" => "Conversation",
        "summarized" => "Summarized",
        "structured" => "Structured",
        other => other,
    }
    .to_string()
}

#[must_use]
fn category_action_hint(id: &str) -> Option<String> {
    match id {
        "system" | "tools" | "rules" | "skills" | "mcp" | "subagents" => {
            Some("Reduce static context to free window".into())
        }
        "conversation" => Some("Older turns will archive via seam or cycle".into()),
        "summarized" => Some("Read-only archived context".into()),
        "structured" => Some("Carried across cycles deterministically".into()),
        _ => None,
    }
}

/// Aggregate compiler spans into Explorer categories (sorted by tokens desc).
#[must_use]
pub fn categories_from_assembly_report(
    report: &ContextAssemblyReport,
    message_count: usize,
) -> Vec<ContextCategory> {
    use std::collections::BTreeMap;

    let mut by_id: BTreeMap<String, u32> = BTreeMap::new();
    for span in &report.spans {
        *by_id.entry(span.category.clone()).or_default() += span.tokens;
    }

    let mut categories: Vec<ContextCategory> = by_id
        .into_iter()
        .map(|(id, tokens)| ContextCategory {
            id: id.clone(),
            label: category_label(&id),
            tokens,
            item_count: if id == "conversation" {
                Some(message_count as u32)
            } else {
                None
            },
            children: None,
            user_action_hint: category_action_hint(&id),
        })
        .collect();
    categories.sort_by(|a, b| b.tokens.cmp(&a.tokens));
    categories
}

/// Build a full breakdown from assembly report + session estimates.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_context_usage_breakdown(
    model: &str,
    assembly_report: Option<&ContextAssemblyReport>,
    estimated_input_tokens: u32,
    context_window_tokens: u32,
    thresholds: &ScaledContextThresholds,
    seam_enabled: bool,
    should_compact: bool,
    message_count: usize,
) -> ContextUsageBreakdown {
    let usage_percent = if context_window_tokens == 0 {
        0.0
    } else {
        ((f64::from(estimated_input_tokens) / f64::from(context_window_tokens)) * 100.0)
            .clamp(0.0, 100.0)
    };

    let categories = if let Some(report) = assembly_report {
        categories_from_assembly_report(report, message_count)
    } else {
        vec![ContextCategory {
            id: "conversation".into(),
            label: category_label("conversation"),
            tokens: estimated_input_tokens,
            item_count: Some(message_count as u32),
            children: None,
            user_action_hint: category_action_hint("conversation"),
        }]
    };

    let next_action = resolve_next_action(
        model,
        u64::from(estimated_input_tokens),
        thresholds,
        seam_enabled,
        should_compact,
    );

    ContextUsageBreakdown {
        model: model.to_string(),
        context_window_tokens,
        estimated_input_tokens,
        usage_percent,
        profile: resolve_context_profile(model).as_str().to_string(),
        next_action,
        categories,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_profile::ContextThresholdOverrides;
    use crate::engine::context_assembly::ContextAssemblyReport;
    use crate::engine::context_compiler::{
        BudgetPolicy, ContextCompiler, ContextLayer, ContextProjection, ContextSource,
        RenderedBlock, SourceId,
    };
    use crate::session::Session;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn v4_thresholds() -> ScaledContextThresholds {
        crate::context_profile::scaled_thresholds(
            "deepseek-v4-pro",
            ContextThresholdOverrides::default(),
        )
    }

    #[test]
    fn next_action_seam_levels_for_large_profile() {
        let thresholds = v4_thresholds();
        assert_eq!(
            resolve_next_action("deepseek-v4-pro", 200_000, &thresholds, true, false),
            ContextNextAction::SeamL1
        );
        assert_eq!(
            resolve_next_action("deepseek-v4-pro", 400_000, &thresholds, true, false),
            ContextNextAction::SeamL2
        );
        assert_eq!(
            resolve_next_action("deepseek-v4-pro", 600_000, &thresholds, true, false),
            ContextNextAction::SeamL3
        );
    }

    #[test]
    fn next_action_cycle_at_scaled_floor() {
        let thresholds = v4_thresholds();
        let headroom = turn_response_headroom_tokens();
        let trigger = cycle_trigger_floor("deepseek-v4-pro", thresholds.cycle, headroom);
        assert_eq!(
            resolve_next_action("deepseek-v4-pro", trigger, &thresholds, true, false),
            ContextNextAction::Cycle
        );
    }

    #[test]
    fn next_action_compact_suggested_for_medium() {
        let thresholds = crate::context_profile::scaled_thresholds(
            "deepseek-chat",
            ContextThresholdOverrides::default(),
        );
        assert_eq!(
            resolve_next_action("deepseek-chat", 50_000, &thresholds, false, true),
            ContextNextAction::CompactSuggested
        );
    }

    #[test]
    fn breakdown_categories_conserve_with_assembly_report() {
        let compiler = ContextCompiler::new()
            .register(ContextSource {
                id: SourceId("system.static"),
                layer: ContextLayer::StaticPrefix,
                priority: 255,
                budget: BudgetPolicy::Fixed(1000),
                render: Arc::new(|_| vec![RenderedBlock::new("system body")]),
            })
            .register(ContextSource {
                id: SourceId("tools.catalog"),
                layer: ContextLayer::StaticPrefix,
                priority: 254,
                budget: BudgetPolicy::Fixed(500),
                render: Arc::new(|_| vec![RenderedBlock::placeholder(500)]),
            });

        let session = Session::new(
            "deepseek-v4-pro".into(),
            PathBuf::from("/tmp"),
            false,
            false,
            PathBuf::from("/tmp/notes.txt"),
            PathBuf::from("/tmp/mcp.json"),
        );
        let compiled = compiler.compile(&ContextProjection::from_session(&session, 0));
        let report = ContextAssemblyReport::from_compiled(&compiled).with_message_tokens(1200);
        let thresholds = v4_thresholds();

        let breakdown = build_context_usage_breakdown(
            "deepseek-v4-pro",
            Some(&report),
            report.estimated_input_tokens,
            1_000_000,
            &thresholds,
            true,
            false,
            3,
        );

        assert_eq!(
            breakdown.category_token_sum(),
            breakdown.estimated_input_tokens
        );
        assert!(breakdown.categories.iter().any(|c| c.id == "system"));
        assert!(breakdown.categories.iter().any(|c| c.id == "conversation"));
    }
}
