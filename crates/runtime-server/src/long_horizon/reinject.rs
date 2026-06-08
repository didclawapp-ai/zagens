//! Periodic objective re-injection from plan + checklist (LHT Phase 2 §4.7).

use zagens_core::chat::{ContentBlock, Message};
use zagens_core::long_horizon::LongHorizonConfig;

use crate::tools::plan::PlanSnapshot;
use crate::tools::todo::TodoListSnapshot;

use super::graph::CodeTaskGraph;
use super::objective::derive_objective;
use super::verify::strip_verify_prefix;

/// Build a user message that re-states the long-horizon objective and open work.
#[must_use]
pub fn build_objective_reinject_message(
    plan: &PlanSnapshot,
    checklist: &TodoListSnapshot,
    messages: &[Message],
    lang: &str,
) -> Option<Message> {
    let graph = CodeTaskGraph::from_snapshots(plan, checklist);
    if graph.is_empty() || !graph.incomplete() {
        return None;
    }
    let (objective, _) = derive_objective(plan, checklist, messages, lang);
    let mut lines = vec![if lang.starts_with("zh") {
        "[长程任务 — 目标重注入]".to_string()
    } else {
        "[Long-horizon task — objective reinject]".to_string()
    }];
    if !objective.is_empty() {
        lines.push(format!("Objective: {objective}"));
    }
    lines.push(format!(
        "Progress: {}% · {} open item(s)",
        graph.completion_pct, graph.open_items
    ));
    for phase in graph
        .phases
        .iter()
        .filter(|p| !matches!(p.status, crate::tools::plan::StepStatus::Completed))
    {
        let marker = match phase.status {
            crate::tools::plan::StepStatus::InProgress => "[~]",
            crate::tools::plan::StepStatus::Pending => "[ ]",
            crate::tools::plan::StepStatus::Completed => "[x]",
        };
        lines.push(format!("- {marker} {}", phase.step));
    }
    for item in graph
        .checklist
        .iter()
        .filter(|c| c.status != crate::tools::todo::TodoStatus::Completed)
    {
        let marker = match item.status {
            crate::tools::todo::TodoStatus::InProgress => "[~]",
            crate::tools::todo::TodoStatus::Pending => "[ ]",
            crate::tools::todo::TodoStatus::Completed => "[x]",
        };
        lines.push(format!(
            "- {marker} #{} {}",
            item.id,
            strip_verify_prefix(&item.content)
        ));
    }
    Some(Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text: lines.join("\n"),
            cache_control: None,
        }],
    })
}

/// Returns true when this assistant step should trigger reinject.
#[must_use]
pub fn should_reinject_this_step(config: &LongHorizonConfig, assistant_steps: u32) -> bool {
    let k = config.reinject_every_steps;
    k > 0 && assistant_steps > 0 && assistant_steps.is_multiple_of(k)
}
