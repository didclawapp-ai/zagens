//! Morning briefing markdown for night queue (Phase 1a.4).

use std::path::Path;

use chrono::Utc;

use zagens_config::workspace_meta_file_write;

use super::model::{BRIEFING_MARKER, NightQueueDocument, QueueTaskStatus};
use super::store;

pub fn render_briefing(doc: &NightQueueDocument) -> String {
    let now = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let mut out = String::new();
    out.push_str(BRIEFING_MARKER);
    out.push_str("\n\n## Night queue briefing\n\n");
    out.push_str(&format!("- **Generated:** {now}\n"));
    if let Some(ts) = doc.last_run_at {
        out.push_str(&format!("- **Last run:** {ts}\n"));
    }

    let passed = doc
        .tasks
        .iter()
        .filter(|t| t.status == QueueTaskStatus::Passed)
        .count();
    let failed = doc
        .tasks
        .iter()
        .filter(|t| {
            matches!(
                t.status,
                QueueTaskStatus::Failed | QueueTaskStatus::RolledBack
            )
        })
        .count();
    let pending = doc
        .tasks
        .iter()
        .filter(|t| t.status == QueueTaskStatus::Pending)
        .count();

    out.push_str(&format!(
        "- **Summary:** {passed} passed · {failed} failed/rolled back · {pending} pending\n\n"
    ));

    if doc.tasks.is_empty() {
        out.push_str("_No tasks in queue._\n");
        return out;
    }

    out.push_str("### Tasks\n\n");
    for task in &doc.tasks {
        let status = format!("{:?}", task.status).to_lowercase();
        out.push_str(&format!("#### `{}` ({status})\n\n", task.id));
        out.push_str(&format!("{}\n\n", store::preview(&task.prompt, 400)));
        if let Some(ref gate) = task.gate_summary {
            out.push_str("**Gate:**\n\n");
            out.push_str(gate);
            out.push('\n');
        }
        if let Some(ref err) = task.error {
            out.push_str(&format!("**Error:** {err}\n\n"));
        }
        if status == "passed" {
            out.push_str("_Merge hint: review worktree diff and open PR when ready._\n\n");
        }
    }

    out
}

pub fn write_briefing_to_handoff(
    workspace: &Path,
    doc: &NightQueueDocument,
) -> std::io::Result<()> {
    let section = render_briefing(doc);
    let path = workspace_meta_file_write(workspace, "handoff.md");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let merged = replace_briefing_block(&existing, &section);
    std::fs::write(path, merged)
}

fn replace_briefing_block(existing: &str, section: &str) -> String {
    if let Some(start) = existing.find(BRIEFING_MARKER) {
        let mut out = String::new();
        let prefix = existing[..start].trim_end();
        if !prefix.is_empty() {
            out.push_str(prefix);
            out.push_str("\n\n");
        }
        out.push_str(section.trim_end());
        out.push('\n');
        return out;
    }

    if existing.trim().is_empty() {
        section.to_string()
    } else {
        format!("{existing}\n\n{section}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn briefing_lists_pass_and_fail() {
        let doc = NightQueueDocument {
            tasks: vec![super::super::model::QueueTask {
                id: "q-1".into(),
                prompt: "fix tests".into(),
                status: QueueTaskStatus::Passed,
                worktree_path: None,
                gate: vec![],
                created_at: Utc::now(),
                started_at: None,
                finished_at: None,
                pre_snapshot_id: None,
                gate_summary: Some("- file_exists: pass".into()),
                error: None,
            }],
            ..Default::default()
        };
        let md = render_briefing(&doc);
        assert!(md.contains("Night queue briefing"));
        assert!(md.contains("q-1"));
    }

    #[test]
    fn replace_briefing_overwrites_prior_block() {
        let first = format!("{BRIEFING_MARKER}\n\n## Night queue briefing\n\nold\n");
        let second = format!("{BRIEFING_MARKER}\n\n## Night queue briefing\n\nnew\n");
        let merged = replace_briefing_block(&first, &second);
        assert_eq!(merged.matches(BRIEFING_MARKER).count(), 1);
        assert!(merged.contains("new"));
        assert!(!merged.contains("old"));
    }
}
