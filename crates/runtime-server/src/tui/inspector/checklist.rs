//! Checklist inspector tab.

use super::super::harness::{ChecklistSnapshot, format_checklist_panel};

pub fn render_panel(snapshot: Option<&ChecklistSnapshot>, height: usize) -> Vec<String> {
    match snapshot {
        Some(snap) => format_checklist_panel(snap, height),
        None => vec![
            "No checklist yet.".to_string(),
            "Prompt the agent to use checklist_write.".to_string(),
        ],
    }
}
