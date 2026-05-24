//! CRAFT P1: File-based blackboard for structured inter-agent context sharing.
//!
//! The blackboard is a JSON file at `.deepseek/blackboards/{task_id}.json`.
//! Each role writes its own partition on completion; the next agent in the
//! task reads the board at spawn time. No live reload — the snapshot is taken
//! once and injected into the child's assignment prompt.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::SubAgentResult;
use super::SubAgentType;

// ── Path helpers ──────────────────────────────────────────────

/// Validate a CRAFT blackboard task id (filename stem under `.deepseek/blackboards/`).
pub fn validate_task_id(task_id: &str) -> Result<(), String> {
    if task_id.is_empty() {
        return Err("task_id 不能为空".to_string());
    }
    if task_id.contains("..") || task_id.contains('/') || task_id.contains('\\') {
        return Err("task_id 含非法路径字符".to_string());
    }
    if !task_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("task_id 仅允许字母、数字、_ 和 -".to_string());
    }
    Ok(())
}

fn blackboard_path(workspace: &Path, task_id: &str) -> Result<PathBuf, String> {
    validate_task_id(task_id)?;
    let mut path = workspace.to_path_buf();
    path.push(".deepseek");
    path.push("blackboards");
    path.push(format!("{task_id}.json"));
    Ok(path)
}

fn ensure_dir(path: &PathBuf) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
}

// ── Public API ─────────────────────────────────────────────────

/// Read the blackboard and format the relevant section as Markdown
/// for injection into the child's assignment prompt.
///
/// Returns `None` if the file doesn't exist or is unreadable.
pub fn read_blackboard_section(
    workspace: &Path,
    task_id: &str,
    agent_type: &SubAgentType,
) -> Option<String> {
    let path = blackboard_path(workspace, task_id).ok()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let board: Value = serde_json::from_str(&raw).ok()?;

    let mut sections: Vec<String> = Vec::new();

    match agent_type {
        SubAgentType::Implementer => {
            // Implementer needs explorer findings + reviewer blockers + verifier failures
            if let Some(s) = format_explorer_findings(&board) {
                sections.push(s);
            }
            if let Some(s) = format_reviewer_blockers(&board) {
                sections.push(s);
            }
            if let Some(s) = format_verifier_failures(&board) {
                sections.push(s);
            }
        }
        SubAgentType::Review => {
            // Reviewer needs implementer changes (to know what was modified)
            if let Some(s) = format_implementer_changes(&board) {
                sections.push(s);
            }
        }
        SubAgentType::Verifier => {
            // Verifier needs implementer changes (to know what to test)
            if let Some(s) = format_implementer_changes(&board) {
                sections.push(s);
            }
        }
        SubAgentType::Auditor => {
            if let Some(s) = format_scratchpad_mirror(&board) {
                sections.push(s);
            }
            if let Some(s) = format_reviewer_blockers(&board) {
                sections.push(s);
            }
        }
        _ => {
            // Explorer, Plan, Custom — no blackboard injection needed
        }
    }

    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n\n"))
    }
}

/// Read the full blackboard as a raw `serde_json::Value`.
/// Returns `None` when the file doesn't exist or is unparseable.
pub fn read_blackboard_raw(workspace: &Path, task_id: &str) -> Option<Value> {
    let path = blackboard_path(workspace, task_id).ok()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// List all task_ids that have a blackboard file under the given workspace.
pub fn list_blackboard_tasks(workspace: &Path) -> Vec<String> {
    let root = workspace.join(".deepseek").join("blackboards");
    let dir = match std::fs::read_dir(&root) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    dir.filter_map(|entry| {
        let entry = entry.ok()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        name.strip_suffix(".json").map(String::from)
    })
    .collect()
}

/// Write one partition to the blackboard for the given agent type.
///
/// Reads the existing board (or starts fresh), updates the matching
/// partition key, and writes back atomically.
pub fn write_blackboard_partition(
    workspace: &Path,
    task_id: &str,
    agent_type: &SubAgentType,
    result: &SubAgentResult,
) {
    let Ok(path) = blackboard_path(workspace, task_id) else {
        return;
    };
    ensure_dir(&path);

    // Read existing board first — needed by Implementer for round tracking
    let existing_raw = std::fs::read_to_string(&path).unwrap_or_default();

    let (partition_key, partition_data) = match agent_type {
        SubAgentType::Explore => {
            // Explorer: write findings from structured verdict (P1+)
            // CRAFT V2: also extract coverage metadata from the output
            ("explorer", json!({
                "findings": build_explorer_findings(result),
                "impact_summary": extract_impact_summary(result),
                "files_examined": extract_files_examined(result),
                "coverage_confidence": extract_coverage_confidence(result),
            }))
        }
        SubAgentType::Implementer => {
            // CRAFT V2: append current round to rounds[] history
            ("implementer", build_implementer_rounds(result, &existing_raw, workspace))
        }
        SubAgentType::Review => {
            ("reviewer", json!({
                "verdict": result.structured_verdict.as_ref()
                    .map(|v| serde_json::to_value(&v.verdict).unwrap_or(json!("PASS")))
                    .unwrap_or(json!("PASS")),
                "blockers": result.structured_verdict.as_ref()
                    .map(|v| &v.items)
                    .unwrap_or(&vec![]),
            }))
        }
        SubAgentType::Verifier => {
            ("verifier", json!({
                "verdict": result.structured_verdict.as_ref()
                    .map(|v| serde_json::to_value(&v.verdict).unwrap_or(json!("PASS")))
                    .unwrap_or(json!("PASS")),
                "failures": build_verifier_failures(result),
                "summary": extract_verifier_summary(result),
            }))
        }
        SubAgentType::Auditor => {
            ("auditor", json!({
                "verdict": result.structured_verdict.as_ref()
                    .map(|v| serde_json::to_value(&v.verdict).unwrap_or(json!("FAIL")))
                    .unwrap_or(json!("FAIL")),
                "details": extract_auditor_details(result),
            }))
        }
        _ => return, // General / Plan / Custom — no blackboard write
    };

    // Merge → write atomically (existing_raw already read above)
    let mut board: Value = if existing_raw.trim().is_empty() {
        json!({
            "schema_version": 1,
            "task_id": task_id,
        })
    } else {
        serde_json::from_str(&existing_raw).unwrap_or(json!({
            "schema_version": 1,
            "task_id": task_id,
        }))
    };

    if let Value::Object(ref mut map) = board {
        map.insert(partition_key.to_string(), partition_data);
    }

    let payload = serde_json::to_string_pretty(&board).unwrap_or_default();
    let tmp_path = path.with_extension("tmp");
    let _ = std::fs::write(&tmp_path, &payload);
    let _ = std::fs::rename(&tmp_path, &path);
}

// ── Markdown formatters (read side) ────────────────────────────

fn format_explorer_findings(board: &Value) -> Option<String> {
    let findings = board.get("explorer")?.get("findings")?.as_array()?;
    if findings.is_empty() {
        return None;
    }
    let mut lines = vec!["### Explorer findings".to_string()];
    for f in findings {
        let file = f.get("file").and_then(|v| v.as_str()).unwrap_or("?");
        let concern = f.get("concern").and_then(|v| v.as_str()).unwrap_or("?");
        let severity = f.get("severity").and_then(|v| v.as_str()).unwrap_or("?");
        let suggestion = f.get("suggestion")
            .and_then(|v| v.as_str())
            .map(|s| format!(" → {s}"))
            .unwrap_or_default();
        lines.push(format!("- [{severity}] `{file}` — {concern}{suggestion}"));
    }
    Some(lines.join("\n"))
}

fn format_reviewer_blockers(board: &Value) -> Option<String> {
    let blockers = board.get("reviewer")?.get("blockers")?.as_array()?;
    if blockers.is_empty() {
        return None;
    }
    let mut lines = vec!["### Reviewer blockers".to_string()];
    for b in blockers {
        let id = b.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let file = b.get("file").and_then(|v| v.as_str()).unwrap_or("?");
        let line = b.get("line").and_then(|v| v.as_u64()).map(|l| format!(":{l}")).unwrap_or_default();
        let desc = b.get("description").and_then(|v| v.as_str()).unwrap_or("?");
        lines.push(format!("- [{id}] `{file}{line}` — {desc}"));
    }
    Some(lines.join("\n"))
}

fn format_verifier_failures(board: &Value) -> Option<String> {
    let failures = board.get("verifier")?.get("failures")?.as_array()?;
    if failures.is_empty() {
        return None;
    }
    let mut lines = vec!["### Verifier failures".to_string()];
    for f in failures {
        let file = f.get("file").and_then(|v| v.as_str()).unwrap_or("?");
        let line = f
            .get("line")
            .and_then(|v| v.as_u64())
            .map(|l| format!(":{l}"))
            .unwrap_or_default();
        let observed = f
            .get("observed")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let hypothesis = f
            .get("hypothesis")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|h| format!(" (hypothesis: {h})"))
            .unwrap_or_default();
        lines.push(format!("- `{file}{line}` — {observed}{hypothesis}"));
    }
    Some(lines.join("\n"))
}

fn format_implementer_changes(board: &Value) -> Option<String> {
    let rounds = board.get("implementer")?.get("rounds")?.as_array()?;
    if rounds.is_empty() {
        return None;
    }
    let mut lines = vec!["### Implementer changes".to_string()];
    for round in rounds {
        let changes = round.get("changes")?.as_array()?;
        for c in changes {
            let file = c.get("file").and_then(|v| v.as_str()).unwrap_or("?");
            let intent = c.get("intent").and_then(|v| v.as_str()).unwrap_or("?");
            lines.push(format!("- `{file}` — {intent}"));
        }
    }
    // Append factual symbol index change record (not model-narrated).
    if let Some(round) = rounds.last() {
        if let Some(sc) = round.get("symbol_changes") {
            if !sc.is_null() {
                lines.push(String::new());
                lines.push("### Symbol index changes (factual)".to_string());
                if let Some(added) = sc.get("added").and_then(|v| v.as_array()) {
                    for a in added {
                        if let Some(s) = a.as_str() {
                            lines.push(format!("- added: `{s}`"));
                        }
                    }
                }
                if let Some(removed) = sc.get("removed").and_then(|v| v.as_array()) {
                    for r in removed {
                        if let Some(s) = r.as_str() {
                            lines.push(format!("- removed: `{s}`"));
                        }
                    }
                }
                if let Some(modified) = sc.get("modified").and_then(|v| v.as_array()) {
                    for m in modified {
                        if let Some(s) = m.as_str() {
                            lines.push(format!("- modified: `{s}`"));
                        }
                    }
                }
            }
        }
    }
    Some(lines.join("\n"))
}

// ── Data extractors (write side) ───────────────────────────────

fn build_explorer_findings(result: &SubAgentResult) -> Value {
    // P1: Explorer may not yet have structured_verdict (P0 only did
    // Reviewer + Verifier). Gracefully return empty when it's None.
    match &result.structured_verdict {
        Some(v) => {
            let items: Vec<Value> = v.items.iter().map(|item| {
                json!({
                    "file": item.file,
                    "concern": item.description,
                    "severity": item.severity,
                    "suggestion": item.suggestion,
                })
            }).collect();
            json!(items)
        }
        None => json!([]),
    }
}

fn extract_impact_summary(result: &SubAgentResult) -> String {
    result
        .structured_verdict
        .as_ref()
        .and_then(|v| v.summary.as_deref())
        .unwrap_or("")
        .to_string()
}

fn build_verifier_failures(result: &SubAgentResult) -> Value {
    let Some(v) = &result.structured_verdict else {
        return json!([]);
    };
    if !matches!(v.verdict, super::VerdictLevel::Fail | super::VerdictLevel::Blocker) {
        return json!([]);
    }
    let failures: Vec<Value> = v
        .items
        .iter()
        .map(|item| {
            json!({
                "observed": item.description,
                "hypothesis": item.suggestion,
                "file": item.file,
                "line": item.line,
                "rule": item.rule,
            })
        })
        .collect();
    json!(failures)
}

fn extract_verifier_summary(result: &SubAgentResult) -> String {
    result
        .structured_verdict
        .as_ref()
        .and_then(|v| v.summary.as_deref())
        .unwrap_or(
            result
                .result
                .as_deref()
                .unwrap_or("")
        )
        .to_string()
}

/// Extract audit failure details from the Auditor's structured_verdict.
/// Falls back to scanning the result text for "FAIL" markers when no
/// structured_verdict is present (graceful degradation for path B).
fn extract_auditor_details(result: &SubAgentResult) -> Value {
    if let Some(v) = &result.structured_verdict {
        json!(v.items)
    } else if let Some(text) = &result.result {
        // Fallback: scan for FAIL markers in the text output
        let details: Vec<Value> = text
            .lines()
            .filter(|l| l.contains("缺失:") || l.contains("FAIL"))
            .map(|l| json!({"detail": l.trim().to_string()}))
            .collect();
        json!(details)
    } else {
        json!([])
    }
}

// ── Phase C3: scratchpad mirror partition ───────────────────────

/// Write read-only scratchpad stats before `agent_spawn(type=auditor)` (§6.12.6).
pub fn write_scratchpad_mirror(
    task_id: &str,
    workspace: &Path,
    run_id: &str,
    config: &crate::scratchpad::ScratchpadConfig,
) {
    let Some(store) = crate::scratchpad::try_open_store(workspace, Some(run_id), None, None) else {
        return;
    };
    let Ok(inventory) = store.read_inventory() else {
        return;
    };
    let Ok(notes) = store.read_notes() else {
        return;
    };
    let stats = crate::scratchpad::compute_coverage_stats(&inventory, &notes, config);
    let superseded = crate::scratchpad::compute_superseded_ids(&notes);
    let high_note_ids: Vec<String> = notes
        .iter()
        .filter(|n| {
            crate::scratchpad::is_verified_finding(n, &superseded)
                && crate::scratchpad::is_high_severity(n.severity.as_deref())
        })
        .map(|n| n.id.clone())
        .collect();

    let partition = json!({
        "run_id": store.run_id(),
        "path": crate::scratchpad::display_run_path(store.run_id()),
        "areas_done": stats.areas_accounted,
        "areas_total": stats.areas_total,
        "findings_verified": stats.verified_findings,
        "high_note_ids": high_note_ids,
    });
    merge_board_partition(workspace, task_id, "scratchpad", partition);
}

fn merge_board_partition(workspace: &Path, task_id: &str, partition_key: &str, partition_data: Value) {
    let Ok(path) = blackboard_path(workspace, task_id) else {
        return;
    };
    ensure_dir(&path);
    let existing_raw = std::fs::read_to_string(&path).unwrap_or_default();
    let mut board: Value = if existing_raw.trim().is_empty() {
        json!({
            "schema_version": 1,
            "task_id": task_id,
        })
    } else {
        serde_json::from_str(&existing_raw).unwrap_or(json!({
            "schema_version": 1,
            "task_id": task_id,
        }))
    };
    if let Value::Object(ref mut map) = board {
        map.insert(partition_key.to_string(), partition_data);
    }
    let payload = serde_json::to_string_pretty(&board).unwrap_or_default();
    let tmp_path = path.with_extension("tmp");
    let _ = std::fs::write(&tmp_path, &payload);
    let _ = std::fs::rename(&tmp_path, &path);
}

fn format_scratchpad_mirror(board: &Value) -> Option<String> {
    let sp = board.get("scratchpad")?;
    let run_id = sp.get("run_id")?.as_str()?;
    let path = sp.get("path").and_then(|v| v.as_str()).unwrap_or("?");
    let done = sp.get("areas_done").and_then(|v| v.as_u64()).unwrap_or(0);
    let total = sp.get("areas_total").and_then(|v| v.as_u64()).unwrap_or(0);
    let verified = sp
        .get("findings_verified")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let highs = sp
        .get("high_note_ids")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    Some(format!(
        "### Scratchpad mirror\n- run_id: `{run_id}`\n- path: `{path}`\n- areas accounted: {done}/{total}\n- verified findings: {verified}\n- high_note_ids: [{highs}]"
    ))
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_workspace() -> PathBuf {
        std::env::temp_dir().join(format!(
            "deepseek-blackboard-test-{}",
            std::process::id()
        ))
    }

    #[test]
    fn test_blackboard_path_contains_task_id() {
        let ws = test_workspace();
        let path = blackboard_path(&ws, "bugfix-001").expect("valid task id");
        let s = path.to_string_lossy();
        assert!(s.contains("bugfix-001"), "path should contain task id, got: {s}");
        assert!(s.ends_with(".json"), "path should end with .json, got: {s}");
        assert!(s.contains(&ws.to_string_lossy().to_string()), "path should be under workspace");
    }

    #[test]
    fn test_blackboard_path_rejects_traversal() {
        let ws = test_workspace();
        assert!(blackboard_path(&ws, "/tmp/evil").is_err());
        assert!(blackboard_path(&ws, "..\\escape").is_err());
    }

    #[test]
    fn test_read_blackboard_returns_none_for_missing_file() {
        let ws = test_workspace();
        let result = read_blackboard_section(&ws, "nonexistent-task-99999", &SubAgentType::Implementer);
        assert!(result.is_none(), "missing file should return None");
    }

    #[test]
    fn test_list_and_read_blackboard_raw() {
        let ws = test_workspace();
        let task_id = "list-test-001";
        let path = blackboard_path(&ws, task_id).expect("valid task id");
        let _ = std::fs::remove_file(&path);
        assert!(list_blackboard_tasks(&ws).is_empty());

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let board = json!({"schema_version": 1, "task_id": task_id});
        std::fs::write(&path, serde_json::to_string_pretty(&board).unwrap()).unwrap();

        let tasks = list_blackboard_tasks(&ws);
        assert!(tasks.contains(&task_id.to_string()));

        let raw = read_blackboard_raw(&ws, task_id).expect("should read board");
        assert_eq!(raw["task_id"], task_id);

        let _ = std::fs::remove_file(&path);
    }

    // ── CRAFT P1 integration tests ──────────────────────────

    use crate::tools::subagent::{
        StructuredVerdict, VerdictItem, VerdictLevel,
        SubAgentResult, SubAgentType as SAT, SubAgentAssignment,
        SubAgentStatus,
    };

    #[test]
    fn test_write_and_read_explorer_findings() {
        let ws = test_workspace();
        let task_id = "test-001";
        let _ = std::fs::remove_file(blackboard_path(&ws, task_id).expect("valid task id"));

        // Simulate Explorer completion with structured_verdict
        let verdict = StructuredVerdict {
            verdict: VerdictLevel::Pass,
            items: vec![
                VerdictItem {
                    severity: "high".into(),
                    file: "auth/login.rs".into(),
                    line: Some(42),
                    description: "token uses standard RNG instead of CSPRNG".into(),
                    rule: Some("TOKEN_INSECURE_RNG".into()),
                    suggestion: Some("replace with OsRng".into()),
                },
                VerdictItem {
                    severity: "medium".into(),
                    file: "auth/session.rs".into(),
                    line: None,
                    description: "session timeout is hardcoded".into(),
                    rule: None,
                    suggestion: Some("make configurable".into()),
                },
            ],
            summary: Some("Two risks in auth module".into()),
        };

        let result = SubAgentResult {
            agent_id: "agent_test".into(),
            agent_type: SAT::Explore,
            assignment: SubAgentAssignment::new("explore auth".into(), None),
            model: "deepseek-v4-flash".into(),
            nickname: None,
            status: SubAgentStatus::Completed,
            result: Some("found two risks".into()),
            steps_taken: 5,
            duration_ms: 1000,
            from_prior_session: false,
            structured_verdict: Some(verdict),
        };

        // Write to blackboard
        write_blackboard_partition(&ws, task_id, &SAT::Explore, &result);

        // Read back as Implementer
        let section = read_blackboard_section(&ws, task_id, &SAT::Implementer)
            .expect("should read back explorer findings for implementer");

        assert!(section.contains("### Explorer findings"), "section: {section}");
        assert!(section.contains("auth/login.rs"), "section: {section}");
        assert!(section.contains("token uses standard RNG"), "section: {section}");
        assert!(section.contains("replace with OsRng"), "section: {section}");
        assert!(section.contains("auth/session.rs"), "section: {section}");
        assert!(section.contains("session timeout"), "section: {section}");

        // Clean up
        let _ = std::fs::remove_file(blackboard_path(&ws, task_id).expect("valid task id"));
    }

    #[test]
    fn test_write_and_read_roundtrip_multiple_roles() {
        let ws = test_workspace();
        let task_id = "test-002";
        let _ = std::fs::remove_file(blackboard_path(&ws, task_id).expect("valid task id"));

        // Write explorer findings
        let explorer_result = SubAgentResult {
            agent_id: "e1".into(),
            agent_type: SAT::Explore,
            assignment: SubAgentAssignment::new("explore".into(), None),
            model: "deepseek-v4-flash".into(),
            nickname: None,
            status: SubAgentStatus::Completed,
            result: Some("done".into()),
            steps_taken: 1,
            duration_ms: 100,
            from_prior_session: false,
            structured_verdict: Some(StructuredVerdict {
                verdict: VerdictLevel::Pass,
                items: vec![VerdictItem {
                    severity: "high".into(),
                    file: "src/main.rs".into(),
                    line: Some(10),
                    description: "unsafe usage".into(),
                    rule: None,
                    suggestion: None,
                }],
                summary: Some("one risk".into()),
            }),
        };
        write_blackboard_partition(&ws, task_id, &SAT::Explore, &explorer_result);

        // Write reviewer blockers
        let reviewer_result = SubAgentResult {
            agent_id: "r1".into(),
            agent_type: SAT::Review,
            assignment: SubAgentAssignment::new("review".into(), None),
            model: "deepseek-v4-flash".into(),
            nickname: None,
            status: SubAgentStatus::Completed,
            result: Some("blocker found".into()),
            steps_taken: 2,
            duration_ms: 200,
            from_prior_session: false,
            structured_verdict: Some(StructuredVerdict {
                verdict: VerdictLevel::Blocker,
                items: vec![VerdictItem {
                    severity: "BLOCKER".into(),
                    file: "src/main.rs".into(),
                    line: Some(15),
                    description: "missing null check".into(),
                    rule: Some("NULL_SAFETY".into()),
                    suggestion: Some("add if guard".into()),
                }],
                summary: Some("one blocker".into()),
            }),
        };
        write_blackboard_partition(&ws, task_id, &SAT::Review, &reviewer_result);

        // Read back as Implementer — should see BOTH
        let section = read_blackboard_section(&ws, task_id, &SAT::Implementer)
            .expect("should read both sections");

        assert!(section.contains("### Explorer findings"), "section: {section}");
        assert!(section.contains("unsafe usage"), "section: {section}");
        assert!(section.contains("### Reviewer blockers"), "section: {section}");
        assert!(section.contains("missing null check"), "section: {section}");

        let _ = std::fs::remove_file(blackboard_path(&ws, task_id).expect("valid task id"));
    }

    #[test]
    fn test_verifier_failures_injected_for_implementer() {
        let ws = test_workspace();
        let task_id = "test-verifier-003";
        let _ = std::fs::remove_file(blackboard_path(&ws, task_id).expect("valid task id"));

        let verifier_result = SubAgentResult {
            agent_id: "v1".into(),
            agent_type: SAT::Verifier,
            assignment: SubAgentAssignment::new("verify".into(), None),
            model: "deepseek-v4-flash".into(),
            nickname: None,
            status: SubAgentStatus::Completed,
            result: Some("tests failed".into()),
            steps_taken: 1,
            duration_ms: 100,
            from_prior_session: false,
            structured_verdict: Some(StructuredVerdict {
                verdict: VerdictLevel::Fail,
                items: vec![VerdictItem {
                    severity: "FAIL".into(),
                    file: "src/lib.rs".into(),
                    line: Some(99),
                    description: "assertion failed".into(),
                    rule: None,
                    suggestion: Some("fix test setup".into()),
                }],
                summary: Some("one failure".into()),
            }),
        };
        write_blackboard_partition(&ws, task_id, &SAT::Verifier, &verifier_result);

        let section = read_blackboard_section(&ws, task_id, &SAT::Implementer)
            .expect("implementer should read verifier failures");
        assert!(section.contains("### Verifier failures"), "{section}");
        assert!(section.contains("assertion failed"), "{section}");
        assert!(section.contains("fix test setup"), "{section}");

        let _ = std::fs::remove_file(blackboard_path(&ws, task_id).expect("valid task id"));
    }
}

fn extract_files_examined(result: &SubAgentResult) -> Value {
    // Parse "## Coverage Report" section from result text
    let text = result.result.as_deref().unwrap_or("");
    let marker = "## Coverage Report";
    if let Some(after) = text.find(marker) {
        let section = &text[after + marker.len()..];
        // Extract "Files examined:" bullet list
        let mut files: Vec<String> = Vec::new();
        let mut in_files = false;
        for line in section.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("- **Files examined**") || trimmed.starts_with("- Files examined") {
                in_files = true;
                continue;
            }
            if in_files {
                if trimmed.starts_with("- ") && !trimmed.starts_with("- **") {
                    let path = trimmed.trim_start_matches("- ").trim();
                    if !path.is_empty() {
                        files.push(path.to_string());
                    }
                } else if trimmed.starts_with("- **") || trimmed.starts_with("##") || trimmed.is_empty() {
                    break;
                }
            }
        }
        json!(files)
    } else {
        json!([])
    }
}

fn extract_coverage_confidence(result: &SubAgentResult) -> Value {
    let text = result.result.as_deref().unwrap_or("");
    let re = regex::Regex::new(r"(?im)^-?\s*\*\*Confidence\*\*:\s*(high|medium|low)").ok();
    if let Some(re) = re {
        if let Some(cap) = re.captures(text) {
            if let Some(m) = cap.get(1) {
                return json!(m.as_str());
            }
        }
    }
    json!("unknown")
}

fn build_implementer_rounds(result: &SubAgentResult, existing_raw: &str, workspace: &Path) -> Value {
    // Read existing rounds, append a new one
    let mut existing_rounds: Vec<Value> = if existing_raw.trim().is_empty() {
        Vec::new()
    } else {
        serde_json::from_str::<Value>(existing_raw)
            .ok()
            .and_then(|v| v.get("implementer").cloned())
            .and_then(|v| v.get("rounds").cloned())
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
    };

    let round_num = existing_rounds.len() + 1;
    let changes = extract_changes_from_result(result);
    let symbol_changes = read_symbol_changes(workspace);

    let new_round = json!({
        "round": round_num,
        "changes": changes,
        "symbol_changes": symbol_changes,
    });

    existing_rounds.push(new_round);
    json!(existing_rounds)
}

fn read_symbol_changes(workspace: &Path) -> Value {
    let path = workspace.join(".deepseek").join(".symbols_changes.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or(json!(null))
}

fn extract_changes_from_result(result: &SubAgentResult) -> Value {
    // Extract changed files from result text (look for path-like references)
    let text = result.result.as_deref().unwrap_or("");
    let re = regex::Regex::new(
        r"(?m)^\s*(?:Modified|Changed|Added|Edited):\s*(.+)$"
    ).ok();
    let mut files: Vec<Value> = Vec::new();
    if let Some(re) = re {
        for cap in re.captures_iter(text) {
            if let Some(m) = cap.get(1) {
                files.push(json!(m.as_str().trim()));
            }
        }
    }
    if files.is_empty() {
        // Fallback: look for file paths in the result
        let path_re = regex::Regex::new(
            r"`(crates/\S+\.(?:rs|toml|ts|tsx|js|json|md))`"
        ).ok();
        if let Some(re) = path_re {
            for cap in re.captures_iter(text) {
                if let Some(m) = cap.get(1) {
                    let path = m.as_str().to_string();
                    if !files.iter().any(|v| v.as_str() == Some(&path)) {
                        files.push(json!(path));
                    }
                }
            }
        }
    }
    json!(files)
}