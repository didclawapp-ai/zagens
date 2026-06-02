//! Merge LHT open-task summary into `.zagens/handoff.md` on cycle advance (Phase 3b).

use deepseek_config::workspace_meta_file_write;

use std::io;
use std::path::Path;

use crate::tools::plan::PlanSnapshot;
use crate::tools::todo::{TodoListSnapshot, TodoStatus};

use super::graph::CodeTaskGraph;

/// Machine-editable region marker — do not remove manually if you rely on auto refresh.
pub const LHT_HANDOFF_MARKER: &str = "<!-- lht-handoff:auto -->";
const LHT_HANDOFF_HEADING: &str = "## Long-horizon task (auto)";

/// Build the auto-maintained handoff section, or `None` when there is nothing to carry.
#[must_use]
pub fn build_lht_handoff_section(
    cycle: u32,
    plan: &PlanSnapshot,
    checklist: &TodoListSnapshot,
) -> Option<String> {
    let graph = CodeTaskGraph::from_snapshots(plan, checklist);
    if graph.is_empty() {
        return None;
    }

    let mut out = String::new();
    out.push_str(LHT_HANDOFF_MARKER);
    out.push('\n');
    out.push_str(LHT_HANDOFF_HEADING);
    out.push_str("\n\n");
    out.push_str(&format!("- **Cycle:** {cycle}\n"));
    out.push_str(&format!(
        "- **Progress:** {}% ({} open)\n",
        graph.completion_pct, graph.open_items
    ));
    if !graph.objective.is_empty() {
        out.push_str(&format!("- **Objective:** {}\n", graph.objective));
    }

    let open_checklist: Vec<_> = graph
        .checklist
        .iter()
        .filter(|c| c.status != TodoStatus::Completed)
        .collect();
    if !open_checklist.is_empty() {
        out.push_str("\n### Open checklist\n");
        for item in open_checklist {
            let marker = match item.status {
                TodoStatus::Pending => "[ ]",
                TodoStatus::InProgress => "[~]",
                TodoStatus::Completed => "[x]",
            };
            out.push_str(&format!("- {marker} #{} {}\n", item.id, item.content));
        }
    }

    let open_phases: Vec<_> = graph
        .phases
        .iter()
        .filter(|p| p.status != crate::tools::plan::StepStatus::Completed)
        .collect();
    if !open_phases.is_empty() {
        out.push_str("\n### Open plan steps\n");
        for phase in open_phases {
            let marker = match phase.status {
                crate::tools::plan::StepStatus::Pending => "[ ]",
                crate::tools::plan::StepStatus::InProgress => "[~]",
                crate::tools::plan::StepStatus::Completed => "[x]",
            };
            out.push_str(&format!("- {marker} {}\n", phase.step));
        }
    }

    if graph.incomplete() {
        out.push_str(
            "\n_Update this file before exiting; the auto block is refreshed on each context cycle._\n",
        );
    } else {
        out.push_str("\n_All tracked plan/checklist items are complete._\n");
    }

    Some(out)
}

/// Replace or append the LHT auto block in the workspace handoff artifact.
pub fn merge_lht_into_handoff(workspace: &Path, section: &str) -> io::Result<()> {
    let path = workspace_meta_file_write(workspace, "handoff.md");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let merged = replace_lht_block(&existing, section);
    std::fs::write(path, merged)
}

fn tail_after_lht_block(rest: &str) -> &str {
    let mut pos = 0;
    while let Some(rel) = rest[pos..].find("\n## ") {
        let abs = pos + rel + 1;
        let line_end = rest[abs..]
            .find('\n')
            .map(|i| abs + i)
            .unwrap_or(rest.len());
        let heading = rest[abs..line_end].trim();
        if heading != LHT_HANDOFF_HEADING {
            return rest[abs..].trim_start();
        }
        pos = abs + 4;
    }
    ""
}

fn replace_lht_block(existing: &str, section: &str) -> String {
    if let Some(start) = existing.find(LHT_HANDOFF_MARKER) {
        let before = existing[..start].trim_end();
        let rest = &existing[start + LHT_HANDOFF_MARKER.len()..];
        let tail = tail_after_lht_block(rest);
        let mut out = String::new();
        if !before.is_empty() {
            out.push_str(before);
            out.push_str("\n\n");
        }
        out.push_str(section.trim());
        if !tail.is_empty() {
            out.push_str("\n\n");
            out.push_str(tail.trim());
        }
        out.push('\n');
        return out;
    }

    if existing.trim().is_empty() {
        format!("# Session handoff\n\n{}\n", section.trim())
    } else {
        format!("{}\n\n{}\n", existing.trim_end(), section.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::plan::{PlanItemArg, PlanSnapshot, StepStatus};
    use crate::tools::todo::{TodoItem, TodoListSnapshot, TodoStatus};

    #[test]
    fn build_section_lists_open_items_and_cycle() {
        let plan = PlanSnapshot {
            explanation: Some("Auth refactor".into()),
            items: vec![PlanItemArg {
                step: "Trait".into(),
                status: StepStatus::InProgress,
            }],
        };
        let checklist = TodoListSnapshot {
            items: vec![
                TodoItem {
                    id: 1,
                    content: "Done".into(),
                    status: TodoStatus::Completed,
                },
                TodoItem {
                    id: 2,
                    content: "Wire tests".into(),
                    status: TodoStatus::Pending,
                },
            ],
            completion_pct: 50,
            in_progress_id: None,
        };
        let section = build_lht_handoff_section(2, &plan, &checklist).expect("section");
        assert!(section.contains("Cycle:** 2"));
        assert!(section.contains("#2 Wire tests"));
        assert!(section.contains("Trait"));
    }

    #[test]
    fn replace_block_is_idempotent() {
        let old = "# Handoff\n\nfoo\n\n<!-- lht-handoff:auto -->\n## Long-horizon task (auto)\n\n- **Cycle:** 1\n\n## User notes\n\nbar\n";
        let new_section = "<!-- lht-handoff:auto -->\n## Long-horizon task (auto)\n\n- **Cycle:** 2\n";
        let merged = replace_lht_block(old, new_section);
        assert!(merged.contains("Cycle:** 2"));
        assert!(merged.contains("User notes"));
        assert!(merged.contains("# Handoff"));
        assert!(!merged.contains("Cycle:** 1"));
    }
}
