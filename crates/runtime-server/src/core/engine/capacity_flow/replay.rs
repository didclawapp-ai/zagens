//! Tool replay selection and canonical-state prompt construction.

use crate::mcp::McpPool;
use crate::models::{ContentBlock, SystemPrompt};

use super::super::tool_catalog::{MULTI_TOOL_PARALLEL_NAME, REQUEST_USER_INPUT_NAME};
use super::super::*;

impl Engine {
    pub(in crate::core::engine) fn select_replay_candidate(
        &self,
        turn: &TurnContext,
        tool_registry: Option<&crate::tools::ToolRegistry>,
    ) -> Option<TurnToolCall> {
        turn.tool_calls
            .iter()
            .rev()
            .find(|call| {
                call.error.is_none()
                    && call.result.is_some()
                    && self.tool_is_replayable_read_only(&call.name, tool_registry)
            })
            .cloned()
    }

    pub(in crate::core::engine) fn tool_is_replayable_read_only(
        &self,
        tool_name: &str,
        tool_registry: Option<&crate::tools::ToolRegistry>,
    ) -> bool {
        if tool_name == MULTI_TOOL_PARALLEL_NAME || tool_name == REQUEST_USER_INPUT_NAME {
            return false;
        }
        if McpPool::is_mcp_tool(tool_name) {
            return mcp_tool_is_read_only(tool_name);
        }
        tool_registry
            .and_then(|registry| registry.get(tool_name))
            .is_some_and(|spec| spec.is_read_only())
    }

    pub(in crate::core::engine) fn build_canonical_state(
        &self,
        turn: &TurnContext,
        note: Option<&str>,
    ) -> CanonicalState {
        let goal = self
            .session
            .messages
            .iter()
            .rev()
            .find_map(|msg| {
                if msg.role != "user" {
                    return None;
                }
                msg.content.iter().find_map(|block| match block {
                    ContentBlock::Text { text, .. } => Some(summarize_text(text, 220)),
                    _ => None,
                })
            })
            .unwrap_or_else(|| "Continue current task from compact state".to_string());

        let mut constraints = vec![
            format!("model={}", self.session.model),
            format!("workspace={}", self.session.workspace.display()),
        ];
        if let Some(note) = note {
            constraints.push(summarize_text(note, 180));
        }

        let mut confirmed_facts = Vec::new();
        for msg in self.session.messages.iter().rev() {
            for block in &msg.content {
                if let ContentBlock::ToolResult { content, .. } = block {
                    if content.starts_with("Error:") {
                        continue;
                    }
                    confirmed_facts.push(summarize_text(content, 180));
                    if confirmed_facts.len() >= 4 {
                        break;
                    }
                }
            }
            if confirmed_facts.len() >= 4 {
                break;
            }
        }

        let open_loops: Vec<String> = turn
            .tool_calls
            .iter()
            .rev()
            .filter_map(|call| {
                call.error
                    .as_ref()
                    .map(|error| format!("{}: {}", call.name, summarize_text(error, 180)))
            })
            .take(4)
            .collect();

        let pending_actions: Vec<String> = if open_loops.is_empty() {
            vec!["Continue with next smallest verifiable step".to_string()]
        } else {
            vec![
                "Re-evaluate failed tool steps with narrower scope".to_string(),
                "Re-derive plan from canonical facts before further edits".to_string(),
            ]
        };

        let mut critical_refs = self.session.working_set.top_paths(8);
        for tool_call in turn.tool_calls.iter().rev().take(4) {
            critical_refs.push(format!("tool:{}", tool_call.id));
        }
        critical_refs.dedup();

        CanonicalState {
            goal,
            constraints,
            confirmed_facts,
            open_loops,
            pending_actions,
            critical_refs,
        }
    }

    /// P4: enrich canonical state with live plan/checklist snapshots (replan path).
    pub(in crate::core::engine) fn build_canonical_state_enriched(
        &self,
        turn: &TurnContext,
        note: Option<&str>,
        plan: &crate::tools::plan::PlanSnapshot,
        checklist: &crate::tools::todo::TodoListSnapshot,
    ) -> CanonicalState {
        let mut canonical = self.build_canonical_state(turn, note);

        if !plan.items.is_empty() || plan.explanation.is_some() {
            canonical
                .constraints
                .push("Active plan (preserve on replan):".to_string());
            if let Some(explanation) = plan.explanation.as_ref().filter(|s| !s.trim().is_empty()) {
                canonical.constraints.push(summarize_text(explanation, 180));
            }
            for item in plan.items.iter().take(8) {
                canonical.constraints.push(format!(
                    "{} {}",
                    item.status.symbol(),
                    summarize_text(&item.step, 160)
                ));
            }
        }

        if !checklist.items.is_empty() {
            canonical.pending_actions = checklist
                .items
                .iter()
                .take(10)
                .map(|item| {
                    format!(
                        "[todo {}] {:?}: {}",
                        item.id,
                        item.status,
                        summarize_text(&item.content, 140)
                    )
                })
                .collect();
        }

        canonical
    }

    pub(in crate::core::engine) fn canonical_prompt(
        &self,
        canonical: &CanonicalState,
        pointer: &str,
        action: GuardrailAction,
        extra: Option<&str>,
    ) -> SystemPrompt {
        let mut lines = vec![
            COMPACTION_SUMMARY_MARKER.to_string(),
            format!("Capacity Canonical State [{}]", action.as_str()),
            format!("Goal: {}", canonical.goal),
            "Constraints:".to_string(),
        ];
        for item in &canonical.constraints {
            lines.push(format!("- {}", summarize_text(item, 200)));
        }
        lines.push("Confirmed Facts:".to_string());
        for item in &canonical.confirmed_facts {
            lines.push(format!("- {}", summarize_text(item, 200)));
        }
        lines.push("Open Loops:".to_string());
        if canonical.open_loops.is_empty() {
            lines.push("- none".to_string());
        } else {
            for item in &canonical.open_loops {
                lines.push(format!("- {}", summarize_text(item, 200)));
            }
        }
        lines.push("Pending Actions:".to_string());
        for item in &canonical.pending_actions {
            lines.push(format!("- {}", summarize_text(item, 200)));
        }
        lines.push("Critical Refs:".to_string());
        for item in &canonical.critical_refs {
            lines.push(format!("- {}", summarize_text(item, 200)));
        }
        if let Some(extra) = extra {
            lines.push(format!("Instruction: {}", summarize_text(extra, 240)));
        }
        lines.push(format!("Memory Pointer: {pointer}"));

        SystemPrompt::Blocks(vec![crate::models::SystemBlock {
            block_type: "text".to_string(),
            text: lines.join("\n"),
            cache_control: None,
        }])
    }
}
