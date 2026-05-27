//! Tool audit gates for scratchpad-backed repo reviews (D16 E1-a2).

use std::path::Path;

use crate::scratchpad::config::ScratchpadConfig;
use crate::scratchpad::coverage::{
    CoverageGateOutcome, build_l0_status_line, compute_coverage_stats, coverage_gate,
    resume_area_id_from_inventory,
};
use crate::scratchpad::path_store::{read_inventory, read_notes, try_open_run_dir};
use crate::scratchpad::schema::AreaStatus;

/// Paths that look like formal audit/code-review deliverables under `deliverables/`.
#[must_use]
pub fn is_audit_deliverable_path(path: &str) -> bool {
    let p = path.replace('\\', "/").to_lowercase();
    if !p.contains("deliverables/") {
        return false;
    }
    p.contains("audit")
        || p.contains("code_review")
        || p.contains("code-review")
        || p.ends_with("_review.md")
        || p.ends_with("/review.md")
}

fn inventory_complete(run_dir: &Path) -> bool {
    let Some(inventory) = read_inventory(run_dir) else {
        return false;
    };
    !inventory.areas.is_empty()
        && inventory.areas.iter().all(|a| {
            matches!(
                a.status,
                AreaStatus::Done | AreaStatus::Deferred
            )
        })
}

/// E5 — block `task_create` while a full-repo audit inventory is active (use `agent_spawn` for P1).
#[must_use]
pub fn check_task_create_audit_gate(
    workspace: &Path,
    run_id: Option<&str>,
) -> Option<String> {
    let run_id = run_id?.trim();
    if run_id.is_empty() {
        return None;
    }
    let (run_dir, run_id) = try_open_run_dir(workspace, Some(run_id))?;
    let inventory = read_inventory(&run_dir)?;
    if inventory.areas.is_empty() {
        return None;
    }
    Some(format!(
        "task_create is blocked during active audit scratchpad run `{run_id}` ({} areas in \
         inventory). For parallel per-area review use `agent_spawn` with `task_id` = that run id, \
         then `agent_result` / `agent_list` to join. `task_read` is only for joining Tasks you \
         already created — do not open new Tasks for area audits. See audit-repo skill P1.",
        inventory.areas.len()
    ))
}

/// Phase C1 / §14 E2 — refuse `write_file` to audit deliverables while scratchpad P2 gates fail.
///
/// Returns an error message when the write must be blocked; `None` when allowed or N/A.
#[must_use]
pub fn check_write_file_audit_report_gate(
    workspace: &Path,
    run_id: Option<&str>,
    config: &ScratchpadConfig,
    path_str: &str,
) -> Option<String> {
    if !config.enabled || !is_audit_deliverable_path(path_str) {
        return None;
    }
    let (run_dir, run) = try_open_run_dir(workspace, run_id)?;
    let inventory = read_inventory(&run_dir)?;
    let notes = read_notes(&run_dir)?;
    let stats = compute_coverage_stats(&inventory, &notes, config);
    let resume = resume_area_id_from_inventory(&inventory);
    let l0 = build_l0_status_line(&run, &stats, &resume);

    if !inventory_complete(&run_dir) {
        return Some(format!(
            "scratchpad audit report write blocked for `{path_str}`: inventory incomplete \
             (every area must be `done` or `deferred` with meta). [{l0}] \
             Use scratchpad_append then scratchpad_set_area before writing to deliverables/."
        ));
    }

    if let CoverageGateOutcome::Block { reason, .. } =
        coverage_gate(&inventory, &notes, config)
    {
        return Some(format!(
            "scratchpad audit report write blocked for `{path_str}`: {reason} [{l0}]"
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn audit_deliverable_path_detection() {
        assert!(is_audit_deliverable_path(
            "deliverables/DS_Pick_Audit_2026-05-20.md"
        ));
        assert!(is_audit_deliverable_path(
            "deliverables/CODE_REVIEW_2026-05-19.md"
        ));
        assert!(!is_audit_deliverable_path("src/main.rs"));
        assert!(!is_audit_deliverable_path("deliverables/notes.txt"));
    }

    #[test]
    fn task_create_gate_blocks_when_inventory_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");
        let run_id = "gate-task";
        let base = ws.join(".deepseek/scratchpad").join(run_id);
        std::fs::create_dir_all(&base).expect("mkdir run");
        let inv = json!({
            "run_id": run_id,
            "areas": [{ "id": "area-a", "path": "src", "status": "pending" }]
        });
        std::fs::write(
            base.join("inventory.json"),
            serde_json::to_string_pretty(&inv).expect("json"),
        )
        .expect("write inv");

        let blocked = check_task_create_audit_gate(&ws, Some(run_id));
        assert!(blocked.is_some());
        assert!(blocked.unwrap().contains("agent_spawn"));
    }

    #[test]
    fn task_create_gate_allows_without_inventory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");
        assert!(check_task_create_audit_gate(&ws, Some("empty-run")).is_none());
        assert!(check_task_create_audit_gate(&ws, None).is_none());
    }

    #[test]
    fn write_file_gate_blocks_incomplete_inventory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");
        let run_id = "gate-write";
        let base = ws.join(".deepseek/scratchpad").join(run_id);
        std::fs::create_dir_all(&base).expect("mkdir run");
        let inv = json!({
            "run_id": run_id,
            "areas": [
                {"id": "a1", "path": "p", "status": "pending", "notes": ""}
            ]
        });
        std::fs::write(
            base.join("inventory.json"),
            serde_json::to_string_pretty(&inv).unwrap(),
        )
        .expect("write inv");
        std::fs::write(base.join("notes.jsonl"), "").expect("notes");

        let cfg = ScratchpadConfig::default();
        let msg = check_write_file_audit_report_gate(
            &ws,
            Some(run_id),
            &cfg,
            "deliverables/Audit.md",
        )
        .expect("blocked");
        assert!(msg.contains("inventory incomplete"));
    }
}
