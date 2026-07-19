//! Tool audit gates for scratchpad-backed repo reviews (D16 E1-a2).

use std::path::Path;

use serde_json::Value;

use crate::scratchpad::config::ScratchpadConfig;
use crate::scratchpad::coverage::{
    CoverageGateOutcome, build_l0_status_line, compute_coverage_stats, coverage_gate,
    resume_area_id_from_inventory,
};
use crate::scratchpad::path_store::{read_inventory, read_notes, try_open_run_dir};
use crate::scratchpad::schema::{AreaStatus, Inventory, NoteLine};

/// Explicit non-audit escape hatch under `deliverables/` (active-run broad gate).
const DELIVERABLES_EXEMPT_MARKERS: &[&str] = &["deliverables/_exempt/", "deliverables/non-audit/"];

/// Paths that look like formal audit/code-review deliverables (heuristic; no active run required).
#[must_use]
pub fn is_audit_deliverable_path(path: &str) -> bool {
    let p_raw = path.replace('\\', "/");
    let p = p_raw.to_lowercase();
    let filename = p.rsplit('/').next().unwrap_or(&p);

    if is_deliverables_gate_exempt(&p) {
        return false;
    }

    // CJK heuristics on the original path (to_lowercase does not alter them).
    if (p_raw.contains("审核") || p_raw.contains("审查"))
        && (p.contains("deliverables/") || p.ends_with(".md") || p.ends_with(".docx"))
    {
        return true;
    }

    if p.contains("deliverables/") {
        return p.contains("deliverables/audit/")
            || p.contains("audit")
            || p.contains("code_review")
            || p.contains("code-review")
            || p.ends_with("_review.md")
            || p.ends_with("/review.md");
    }

    if filename.starts_with("code_audit") && filename.ends_with(".md") {
        return true;
    }

    if (p.contains("/doc/") || p.starts_with("doc/")) && p.contains("audit") && p.ends_with(".md") {
        return true;
    }

    false
}

/// `deliverables/**` document extensions gated while an audit scratchpad run is active.
#[must_use]
pub fn is_deliverables_document_path(path: &str) -> bool {
    let p = path.replace('\\', "/").to_lowercase();
    if !p.contains("deliverables/") || is_deliverables_gate_exempt(&p) {
        return false;
    }
    p.ends_with(".md")
        || p.ends_with(".docx")
        || p.ends_with(".xlsx")
        || p.ends_with(".pptx")
        || p.ends_with(".pdf")
}

fn is_deliverables_gate_exempt(path_normalized: &str) -> bool {
    DELIVERABLES_EXEMPT_MARKERS
        .iter()
        .any(|m| path_normalized.contains(m))
}

fn inventory_complete(inventory: &Inventory) -> bool {
    !inventory.areas.is_empty()
        && inventory
            .areas
            .iter()
            .all(|a| matches!(a.status, AreaStatus::Done | AreaStatus::Deferred))
}

fn unfinished_area_ids(inventory: &Inventory) -> Vec<String> {
    inventory
        .areas
        .iter()
        .filter(|a| !matches!(a.status, AreaStatus::Done | AreaStatus::Deferred))
        .map(|a| a.id.clone())
        .collect()
}

/// Staged intermediate report path (filename or `deliverables/audit/staged/`).
#[must_use]
pub fn is_staged_report_path(path: &str) -> bool {
    let p = path.replace('\\', "/").to_lowercase();
    if !(p.contains("deliverables/") && (p.ends_with(".md") || p.ends_with(".docx"))) {
        return false;
    }
    if p.contains("/staged/") {
        return true;
    }
    let filename = p.rsplit('/').next().unwrap_or("");
    filename.contains("staged")
}

#[must_use]
pub fn staged_report_approved(notes: &[NoteLine]) -> bool {
    notes.iter().any(|n| {
        n.area_id == "_global"
            && n.kind == "meta"
            && n.claim.as_ref().is_some_and(|c| {
                let lower = c.to_lowercase();
                lower.contains("staged_report") || lower.contains("分阶段报告")
            })
    })
}

/// E5 — block `task_create` while a full-repo audit inventory is active (use `agent_spawn` for P1).
#[must_use]
pub fn check_task_create_audit_gate(workspace: &Path, run_id: Option<&str>) -> Option<String> {
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
/// When an active audit run is bound, all `deliverables/**` documents are gated (unless exempt).
/// Heuristic audit paths (ASCII `audit`, CJK 审核/审查, …) are gated whenever a run can be opened.
///
/// Returns an error message when the write must be blocked; `None` when allowed or N/A.
#[must_use]
pub fn check_write_file_audit_report_gate(
    workspace: &Path,
    run_id: Option<&str>,
    config: &ScratchpadConfig,
    path_str: &str,
) -> Option<String> {
    if !config.enabled {
        return None;
    }

    let (run_dir, run) = try_open_run_dir(workspace, run_id)?;
    let inventory = read_inventory(&run_dir)?;
    if inventory.areas.is_empty() {
        return None;
    }

    let heuristic = is_audit_deliverable_path(path_str);
    let active_deliverables = is_deliverables_document_path(path_str);
    if !heuristic && !active_deliverables {
        return None;
    }

    let notes = read_notes(&run_dir)?;
    let stats = compute_coverage_stats(&inventory, &notes, config);
    let resume = resume_area_id_from_inventory(&inventory);
    let l0 = build_l0_status_line(&run, &stats, &resume);

    // S1.d — staged intermediate report (path + _global meta + reviewed_ratio).
    if is_staged_report_path(path_str) {
        if !staged_report_approved(&notes) {
            return Some(format!(
                "scratchpad staged report write blocked for `{path_str}`: append `_global` meta \
                 with `staged_report` / `分阶段报告` first. [{l0}]"
            ));
        }
        if stats.reviewed_ratio < config.coverage_reviewed_hard_ratio {
            return Some(format!(
                "scratchpad staged report write blocked for `{path_str}`: reviewed_ratio {:.0}% \
                 is below {:.0}% (need more done areas with finding/cleared). [{l0}]",
                stats.reviewed_ratio * 100.0,
                config.coverage_reviewed_hard_ratio * 100.0,
            ));
        }
        return None;
    }

    if !inventory_complete(&inventory) {
        let unfinished = unfinished_area_ids(&inventory);
        let listed = unfinished
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let more = if unfinished.len() > 8 {
            format!(" … +{} more", unfinished.len() - 8)
        } else {
            String::new()
        };
        return Some(format!(
            "scratchpad audit report write blocked for `{path_str}`: inventory incomplete \
             (every area must be `done` or `deferred` with meta). \
             pending_area_ids ({}) e.g. [{listed}{more}]. [{l0}] \
             Prefer scratchpad_defer_remaining for mass closeout, or finish areas as done. \
             For an intermediate draft use a STAGED path under deliverables/audit/staged/ \
             with `_global` meta staged_report (reviewed_ratio ≥ 40%).",
            unfinished.len()
        ));
    }

    if let CoverageGateOutcome::Block { reason, .. } = coverage_gate(&inventory, &notes, config) {
        return Some(format!(
            "scratchpad audit report write blocked for `{path_str}`: {reason} [{l0}]"
        ));
    }

    // Force structured import — hand-copying explore/review prose bypasses verification.
    if let Some(msg) = check_import_agent_gate(workspace, &run, &notes, path_str, &l0) {
        return Some(msg);
    }

    None
}

fn check_import_agent_gate(
    workspace: &Path,
    run_id: &str,
    notes: &[NoteLine],
    path_str: &str,
    l0: &str,
) -> Option<String> {
    let (pending, running) = audit_agents_pending_import(workspace, run_id, notes);
    if pending.is_empty() && running.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !running.is_empty() {
        let listed = running
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let more = if running.len() > 6 {
            format!(" … +{} more", running.len() - 6)
        } else {
            String::new()
        };
        parts.push(format!(
            "wait for explore/review agents still running ({}) e.g. [{listed}{more}]",
            running.len()
        ));
    }
    if !pending.is_empty() {
        let listed = pending
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let more = if pending.len() > 6 {
            format!(" … +{} more", pending.len() - 6)
        } else {
            String::new()
        };
        parts.push(format!(
            "call `scratchpad_import_agent` for completed explore/review agents ({}) e.g. [{listed}{more}] \
             (do not hand-copy prose; missing <!-- audit-findings --> → re-spawn then import)",
            pending.len()
        ));
    }
    Some(format!(
        "scratchpad audit report write blocked for `{path_str}`: {}; then retry write_file. [{l0}]",
        parts.join("; ")
    ))
}

/// Explore/Review agents bound to this audit run that must be `scratchpad_import_agent`'d
/// before the final report (or still running).
#[must_use]
pub fn audit_agents_pending_import(
    workspace: &Path,
    run_id: &str,
    notes: &[NoteLine],
) -> (Vec<String>, Vec<String>) {
    let mut pending_import = Vec::new();
    let mut still_running = Vec::new();
    let Some(raw) = read_subagents_state_json(workspace) else {
        return (pending_import, still_running);
    };
    let Some(agents) = raw.get("agents").and_then(|v| v.as_array()) else {
        return (pending_import, still_running);
    };

    for agent in agents {
        if !agent_bound_to_run(agent, run_id) {
            continue;
        }
        if !is_audit_import_agent_type(agent) {
            continue;
        }
        let Some(id) = agent.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        if agent_status_is_running(agent) {
            still_running.push(id.to_string());
            continue;
        }
        if !agent_status_is_completed(agent) {
            continue;
        }
        if !agent_completion_importable(agent) {
            continue;
        }
        if !notes_import_agent(notes, id) {
            pending_import.push(id.to_string());
        }
    }
    (pending_import, still_running)
}

fn read_subagents_state_json(workspace: &Path) -> Option<Value> {
    let path = zagens_config::workspace_meta_file_read(workspace, "state/subagents.v1.json");
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn agent_bound_to_run(agent: &Value, run_id: &str) -> bool {
    agent
        .get("scratchpad_run_id")
        .and_then(|v| v.as_str())
        .is_some_and(|bound| bound == run_id)
}

fn is_audit_import_agent_type(agent: &Value) -> bool {
    matches!(
        agent.get("agent_type").and_then(|v| v.as_str()),
        Some("explore" | "review")
    )
}

fn agent_status_is_running(agent: &Value) -> bool {
    match agent.get("status") {
        Some(Value::String(s)) => s.eq_ignore_ascii_case("running"),
        _ => false,
    }
}

fn agent_status_is_completed(agent: &Value) -> bool {
    match agent.get("status") {
        Some(Value::String(s)) => s.eq_ignore_ascii_case("completed"),
        _ => false,
    }
}

/// Import tool only accepts NaturalBreak (or missing reason on older records).
fn agent_completion_importable(agent: &Value) -> bool {
    match agent.get("completion_reason") {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => {
            s.eq_ignore_ascii_case("NaturalBreak") || s.eq_ignore_ascii_case("natural_break")
        }
        Some(Value::Object(map)) => map.contains_key("NaturalBreak"),
        _ => false,
    }
}

#[must_use]
pub fn notes_import_agent(notes: &[NoteLine], agent_id: &str) -> bool {
    let source = format!("agent:{agent_id}");
    let receipt = format!("imported_agent:{agent_id}");
    notes.iter().any(|n| {
        n.source.as_deref() == Some(source.as_str())
            || n.claim
                .as_deref()
                .is_some_and(|c| c.contains(receipt.as_str()) || c.contains(source.as_str()))
    })
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
        assert!(is_audit_deliverable_path(
            "doc/CODE_AUDIT_REPORT-v2.67.0.md"
        ));
        assert!(is_audit_deliverable_path("doc/code_audit_summary.md"));
        assert!(is_audit_deliverable_path(
            "deliverables/Zagens_代码审核报告_2026-07-19.md"
        ));
        assert!(is_audit_deliverable_path(
            "deliverables/audit/Zagens_code_audit_2026-07-19.md"
        ));
        assert!(!is_audit_deliverable_path("src/main.rs"));
        assert!(!is_audit_deliverable_path("deliverables/notes.txt"));
        assert!(!is_audit_deliverable_path("doc/README.md"));
        assert!(!is_audit_deliverable_path("deliverables/_exempt/slides.md"));
    }

    #[test]
    fn deliverables_document_gated_under_active_run_even_without_audit_token() {
        assert!(is_deliverables_document_path(
            "deliverables/meeting-notes.md"
        ));
        assert!(!is_deliverables_document_path(
            "deliverables/non-audit/notes.md"
        ));
        assert!(!is_deliverables_document_path("src/readme.md"));
    }

    #[test]
    fn task_create_gate_blocks_when_inventory_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");
        let run_id = "gate-task";
        let base = zagens_config::workspace_meta_dir(&ws)
            .join("scratchpad")
            .join(run_id);
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

    fn write_incomplete_run(ws: &Path, run_id: &str) {
        let base = zagens_config::workspace_meta_dir(ws)
            .join("scratchpad")
            .join(run_id);
        std::fs::create_dir_all(&base).expect("mkdir run");
        let inv = json!({
            "run_id": run_id,
            "areas": [
                {"id": "a1", "path": "p", "status": "pending", "notes": ""},
                {"id": "a2", "path": "q", "status": "in_progress", "notes": ""}
            ]
        });
        std::fs::write(
            base.join("inventory.json"),
            serde_json::to_string_pretty(&inv).unwrap(),
        )
        .expect("write inv");
        std::fs::write(base.join("notes.jsonl"), "").expect("notes");
    }

    #[test]
    fn write_file_gate_blocks_incomplete_inventory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");
        let run_id = "gate-write";
        write_incomplete_run(&ws, run_id);

        let cfg = ScratchpadConfig::default();
        let msg =
            check_write_file_audit_report_gate(&ws, Some(run_id), &cfg, "deliverables/Audit.md")
                .expect("blocked");
        assert!(msg.contains("inventory incomplete"));
        assert!(msg.contains("pending_area_ids"));
        assert!(msg.contains("a1"));
    }

    #[test]
    fn write_file_gate_blocks_chinese_audit_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");
        let run_id = "gate-zh";
        write_incomplete_run(&ws, run_id);

        let cfg = ScratchpadConfig::default();
        let msg = check_write_file_audit_report_gate(
            &ws,
            Some(run_id),
            &cfg,
            "deliverables/Zagens_代码审核报告_2026-07-19.md",
        )
        .expect("blocked");
        assert!(msg.contains("inventory incomplete"), "{msg}");
    }

    #[test]
    fn write_file_gate_blocks_plain_deliverables_md_when_run_active() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");
        let run_id = "gate-plain";
        write_incomplete_run(&ws, run_id);

        let cfg = ScratchpadConfig::default();
        let msg = check_write_file_audit_report_gate(
            &ws,
            Some(run_id),
            &cfg,
            "deliverables/meeting-notes.md",
        )
        .expect("blocked");
        assert!(msg.contains("inventory incomplete"), "{msg}");
    }

    #[test]
    fn write_file_gate_allows_exempt_deliverables_when_incomplete() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");
        let run_id = "gate-exempt";
        write_incomplete_run(&ws, run_id);

        let cfg = ScratchpadConfig::default();
        assert!(
            check_write_file_audit_report_gate(
                &ws,
                Some(run_id),
                &cfg,
                "deliverables/_exempt/deck.md",
            )
            .is_none()
        );
    }

    fn write_reviewed_incomplete_run(ws: &Path, run_id: &str) {
        let base = zagens_config::workspace_meta_dir(ws)
            .join("scratchpad")
            .join(run_id);
        std::fs::create_dir_all(&base).expect("mkdir run");
        // 2/5 done with cleared → reviewed_ratio 40%; 3 pending.
        let inv = json!({
            "run_id": run_id,
            "areas": [
                {"id": "a1", "path": "p1", "status": "done", "notes": ""},
                {"id": "a2", "path": "p2", "status": "done", "notes": ""},
                {"id": "a3", "path": "p3", "status": "pending", "notes": ""},
                {"id": "a4", "path": "p4", "status": "pending", "notes": ""},
                {"id": "a5", "path": "p5", "status": "pending", "notes": ""}
            ]
        });
        std::fs::write(
            base.join("inventory.json"),
            serde_json::to_string_pretty(&inv).unwrap(),
        )
        .expect("write inv");
        let notes = concat!(
            r#"{"id":"n1","area_id":"a1","kind":"cleared","claim":"[D2] read_file sampled — no correctness regressions in examined paths"}"#,
            "\n",
            r#"{"id":"n2","area_id":"a2","kind":"cleared","claim":"[D2] grep_files sampled — error paths look consistent"}"#,
            "\n",
            r#"{"id":"n3","area_id":"_global","kind":"meta","claim":"staged_report intermediate draft"}"#,
            "\n",
        );
        std::fs::write(base.join("notes.jsonl"), notes).expect("notes");
    }

    #[test]
    fn staged_path_allowed_when_meta_and_reviewed_ratio() {
        assert!(is_staged_report_path(
            "deliverables/audit/staged/Zagens_STAGED_40.md"
        ));
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");
        let run_id = "gate-staged";
        write_reviewed_incomplete_run(&ws, run_id);
        let cfg = ScratchpadConfig::default();
        assert!(
            check_write_file_audit_report_gate(
                &ws,
                Some(run_id),
                &cfg,
                "deliverables/audit/staged/Zagens_STAGED_40.md",
            )
            .is_none()
        );
    }

    #[test]
    fn final_report_still_blocked_when_staged_would_pass() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");
        let run_id = "gate-final";
        write_reviewed_incomplete_run(&ws, run_id);
        let cfg = ScratchpadConfig::default();
        let msg = check_write_file_audit_report_gate(
            &ws,
            Some(run_id),
            &cfg,
            "deliverables/audit/Zagens_code_audit_final.md",
        )
        .expect("blocked");
        assert!(msg.contains("inventory incomplete"), "{msg}");
    }

    fn write_complete_reviewed_run(ws: &Path, run_id: &str) {
        let base = zagens_config::workspace_meta_dir(ws)
            .join("scratchpad")
            .join(run_id);
        std::fs::create_dir_all(&base).expect("mkdir run");
        let inv = json!({
            "run_id": run_id,
            "areas": [
                {"id": "a1", "path": "p1", "status": "done", "notes": ""},
                {"id": "a2", "path": "p2", "status": "done", "notes": ""},
                {"id": "a3", "path": "p3", "status": "deferred", "notes": ""},
                {"id": "a4", "path": "p4", "status": "deferred", "notes": ""},
                {"id": "a5", "path": "p5", "status": "deferred", "notes": ""}
            ]
        });
        std::fs::write(
            base.join("inventory.json"),
            serde_json::to_string_pretty(&inv).unwrap(),
        )
        .expect("write inv");
        let notes = concat!(
            r#"{"id":"n1","area_id":"a1","kind":"cleared","claim":"[D2] read_file sampled — no correctness regressions in examined paths"}"#,
            "\n",
            r#"{"id":"n2","area_id":"a2","kind":"cleared","claim":"[D2] grep_files sampled — error paths look consistent"}"#,
            "\n",
            r#"{"id":"n3","area_id":"a3","kind":"meta","claim":"deferred: overlap with a1/a2 unreviewed remainder"}"#,
            "\n",
            r#"{"id":"n4","area_id":"a4","kind":"meta","claim":"deferred: overlap with a1/a2 unreviewed remainder"}"#,
            "\n",
            r#"{"id":"n5","area_id":"a5","kind":"meta","claim":"deferred: overlap with a1/a2 unreviewed remainder"}"#,
            "\n",
        );
        std::fs::write(base.join("notes.jsonl"), notes).expect("notes");
    }

    fn write_subagents_state(ws: &Path, body: &Value) {
        let state_dir = zagens_config::workspace_meta_dir(ws).join("state");
        std::fs::create_dir_all(&state_dir).expect("state dir");
        std::fs::write(
            state_dir.join("subagents.v1.json"),
            serde_json::to_string_pretty(body).unwrap(),
        )
        .expect("write subagents");
    }

    #[test]
    fn write_file_gate_blocks_unimported_explore_agents() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");
        let run_id = "gate-import";
        write_complete_reviewed_run(&ws, run_id);
        write_subagents_state(
            &ws,
            &json!({
                "schema_version": 1,
                "agents": [{
                    "id": "agent_abc",
                    "agent_type": "explore",
                    "status": "Completed",
                    "completion_reason": "NaturalBreak",
                    "scratchpad_run_id": run_id,
                    "prompt": "x",
                    "assignment": {"objective": "audit"},
                    "steps_taken": 1,
                    "duration_ms": 1,
                    "allowed_tools": [],
                    "updated_at_ms": 1
                }]
            }),
        );

        let cfg = ScratchpadConfig::default();
        let msg = check_write_file_audit_report_gate(
            &ws,
            Some(run_id),
            &cfg,
            "deliverables/audit/Zagens_code_audit_final.md",
        )
        .expect("blocked");
        assert!(msg.contains("scratchpad_import_agent"), "{msg}");
        assert!(msg.contains("agent_abc"), "{msg}");
    }

    #[test]
    fn write_file_gate_allows_after_import_receipt() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");
        let run_id = "gate-import-ok";
        write_complete_reviewed_run(&ws, run_id);
        write_subagents_state(
            &ws,
            &json!({
                "schema_version": 1,
                "agents": [{
                    "id": "agent_ok",
                    "agent_type": "explore",
                    "status": "Completed",
                    "completion_reason": "NaturalBreak",
                    "scratchpad_run_id": run_id,
                    "prompt": "x",
                    "assignment": {"objective": "audit"},
                    "steps_taken": 1,
                    "duration_ms": 1,
                    "allowed_tools": [],
                    "updated_at_ms": 1
                }]
            }),
        );
        let notes_path = zagens_config::workspace_meta_dir(&ws)
            .join("scratchpad")
            .join(run_id)
            .join("notes.jsonl");
        let mut notes = std::fs::read_to_string(&notes_path).unwrap();
        notes.push_str(
            r#"{"id":"imp","area_id":"_global","kind":"meta","claim":"imported_agent:agent_ok","source":"agent:agent_ok"}
"#,
        );
        std::fs::write(&notes_path, notes).unwrap();

        let cfg = ScratchpadConfig::default();
        assert!(
            check_write_file_audit_report_gate(
                &ws,
                Some(run_id),
                &cfg,
                "deliverables/audit/Zagens_code_audit_final.md",
            )
            .is_none()
        );
    }
}
