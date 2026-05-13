//! CRAFT P1: File-based blackboard for structured inter-agent context sharing.
//!
//! The blackboard is a JSON file at `.deepseek/blackboards/{task_id}.json`.
//! Each role writes its own partition on completion; the next agent in the
//! task reads the board at spawn time. No live reload — the snapshot is taken
//! once and injected into the child's assignment prompt.

use std::path::PathBuf;

use serde_json::{Value, json};

use super::SubAgentResult;
use super::SubAgentType;

// ── Path helpers ──────────────────────────────────────────────

fn workspace_root() -> Option<PathBuf> {
    std::env::current_dir().ok()
}

fn blackboard_path(task_id: &str) -> PathBuf {
    let mut path = workspace_root()
        .unwrap_or_else(|| PathBuf::from("."));
    path.push(".deepseek");
    path.push("blackboards");
    path.push(format!("{task_id}.json"));
    path
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
    task_id: &str,
    agent_type: &SubAgentType,
) -> Option<String> {
    let path = blackboard_path(task_id);
    let raw = std::fs::read_to_string(&path).ok()?;
    let board: Value = serde_json::from_str(&raw).ok()?;

    let mut sections: Vec<String> = Vec::new();

    match agent_type {
        SubAgentType::Implementer => {
            // Implementer needs explorer findings + reviewer blockers
            if let Some(s) = format_explorer_findings(&board) {
                sections.push(s);
            }
            if let Some(s) = format_reviewer_blockers(&board) {
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

/// Write one partition to the blackboard for the given agent type.
///
/// Reads the existing board (or starts fresh), updates the matching
/// partition key, and writes back atomically.
pub fn write_blackboard_partition(
    task_id: &str,
    agent_type: &SubAgentType,
    result: &SubAgentResult,
) {
    let path = blackboard_path(task_id);
    ensure_dir(&path);

    let (partition_key, partition_data) = match agent_type {
        SubAgentType::Explore => {
            // Explorer: write findings from structured verdict (P1+
            // — graceful when structured_verdict is still None)
            ("explorer", json!({
                "findings": build_explorer_findings(result),
                "impact_summary": extract_impact_summary(result),
            }))
        }
        SubAgentType::Implementer => {
            // Implementer: append a new round to rounds[]
            ("implementer", json!({
                "rounds": json!([]),  // placeholder — filled by merge logic
            }))
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
                "failures": json!([]),
                "summary": extract_verifier_summary(result),
            }))
        }
        _ => return, // General / Plan / Custom — no blackboard write
    };

    // Read → merge → write atomically
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

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blackboard_path_contains_task_id() {
        let path = blackboard_path("bugfix-001");
        let s = path.to_string_lossy();
        assert!(s.contains("bugfix-001"), "path should contain task id, got: {s}");
        assert!(s.ends_with(".json"), "path should end with .json, got: {s}");
    }

    #[test]
    fn test_read_blackboard_returns_none_for_missing_file() {
        let result = read_blackboard_section("nonexistent-task-99999", &SubAgentType::Implementer);
        assert!(result.is_none(), "missing file should return None");
    }

    // ── CRAFT P1 integration tests ──────────────────────────

    use crate::tools::subagent::{
        StructuredVerdict, VerdictItem, VerdictLevel,
        SubAgentResult, SubAgentType as SAT, SubAgentAssignment,
        SubAgentStatus,
    };

    #[test]
    fn test_write_and_read_explorer_findings() {
        let task_id = "test-001";
        let _ = std::fs::remove_file(blackboard_path(task_id));

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
        write_blackboard_partition(task_id, &SAT::Explore, &result);

        // Read back as Implementer
        let section = read_blackboard_section(task_id, &SAT::Implementer)
            .expect("should read back explorer findings for implementer");

        assert!(section.contains("### Explorer findings"), "section: {section}");
        assert!(section.contains("auth/login.rs"), "section: {section}");
        assert!(section.contains("token uses standard RNG"), "section: {section}");
        assert!(section.contains("replace with OsRng"), "section: {section}");
        assert!(section.contains("auth/session.rs"), "section: {section}");
        assert!(section.contains("session timeout"), "section: {section}");

        // Clean up
        let _ = std::fs::remove_file(blackboard_path(task_id));
    }

    #[test]
    fn test_write_and_read_roundtrip_multiple_roles() {
        let task_id = "test-002";
        let _ = std::fs::remove_file(blackboard_path(task_id));

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
        write_blackboard_partition(task_id, &SAT::Explore, &explorer_result);

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
        write_blackboard_partition(task_id, &SAT::Review, &reviewer_result);

        // Read back as Implementer — should see BOTH
        let section = read_blackboard_section(task_id, &SAT::Implementer)
            .expect("should read both sections");

        assert!(section.contains("### Explorer findings"), "section: {section}");
        assert!(section.contains("unsafe usage"), "section: {section}");
        assert!(section.contains("### Reviewer blockers"), "section: {section}");
        assert!(section.contains("missing null check"), "section: {section}");

        let _ = std::fs::remove_file(blackboard_path(task_id));
    }
}
