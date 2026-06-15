//! CRAFT P1: File-based blackboard for structured inter-agent context sharing.
//!
//! The blackboard is a JSON file at `.zagens/blackboards/{task_id}.json`.
//! Each role writes its own partition on completion; the next agent in the
//! task reads the board at spawn time. No live reload — the snapshot is taken
//! once and injected into the child's assignment prompt.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use zagens_config::{
    legacy_workspace_meta_dir, workspace_meta_dir, workspace_meta_file_read,
    workspace_meta_file_write,
};

use serde_json::{Value, json};

use zagens_core::subagent::{StructuredFindings, StructuredVerdict, SubAgentType};

use super::SubAgentResult;
use super::blackboard_cache;

// ── Path helpers ──────────────────────────────────────────────

/// Validate a CRAFT blackboard task id (filename stem under `.zagens/blackboards/`).
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

fn blackboard_rel(task_id: &str) -> Result<String, String> {
    validate_task_id(task_id)?;
    Ok(format!("blackboards/{task_id}.json"))
}

fn blackboard_path_read(workspace: &Path, task_id: &str) -> Result<PathBuf, String> {
    Ok(workspace_meta_file_read(
        workspace,
        &blackboard_rel(task_id)?,
    ))
}

fn blackboard_path_write(workspace: &Path, task_id: &str) -> Result<PathBuf, String> {
    Ok(workspace_meta_file_write(
        workspace,
        &blackboard_rel(task_id)?,
    ))
}

fn ensure_dir(path: &Path) {
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
    let path = blackboard_path_read(workspace, task_id).ok()?;
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
            // C6(LSP): include cargo check diagnostics if available
            if let Some(s) =
                crate::tools::subagent::craft::lsp_post_hook::format_lsp_diagnostics(&board)
            {
                sections.push(s);
            }
            // C11: requirements traceability report (omitted when no requirements registered)
            if let Some(task_id) = board.get("task_id").and_then(|v| v.as_str()) {
                // checklist items are not available here; we surface the raw
                // requirements list so the Reviewer can cross-check manually.
                let reqs = board
                    .get("requirements")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|e| {
                                let id = e.get("id")?.as_str()?.trim().to_string();
                                let text = e.get("text")?.as_str()?.trim().to_string();
                                if id.is_empty() {
                                    None
                                } else {
                                    Some(RequirementEntry { id, text })
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if !reqs.is_empty() {
                    let _ = task_id; // task_id already consumed above for the check
                    let mut lines = vec!["### Requirements to verify against".to_string()];
                    for r in &reqs {
                        lines.push(format!("- `{}`: {}", r.id, r.text));
                    }
                    lines.push(String::new());
                    lines.push(
                        "> Check each requirement above is addressed by the implementation. \
                         Raise a BLOCKER for any requirement with no evidence of coverage."
                            .to_string(),
                    );
                    sections.push(lines.join("\n"));
                }
            }
        }
        SubAgentType::Verifier => {
            // Verifier needs implementer changes (to know what to test)
            if let Some(s) = format_implementer_changes(&board) {
                sections.push(s);
            }
            // C6(LSP): include cargo check diagnostics for verifier context
            if let Some(s) =
                crate::tools::subagent::craft::lsp_post_hook::format_lsp_diagnostics(&board)
            {
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
    let path = blackboard_path_read(workspace, task_id).ok()?;
    let raw = blackboard_cache::read_cached(&path)?;
    serde_json::from_str(&raw).ok()
}

/// List all task_ids that have a blackboard file under the given workspace.
pub fn list_blackboard_tasks(workspace: &Path) -> Vec<String> {
    let mut tasks = std::collections::BTreeSet::new();
    for root in [
        workspace_meta_dir(workspace).join("blackboards"),
        legacy_workspace_meta_dir(workspace).join("blackboards"),
    ] {
        let Ok(dir) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in dir.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(task_id) = name.strip_suffix(".json") {
                tasks.insert(task_id.to_string());
            }
        }
    }
    tasks.into_iter().collect()
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
    let Ok(path) = blackboard_path_write(workspace, task_id) else {
        return;
    };
    ensure_dir(&path);

    // Read existing board first — needed by Implementer for round tracking
    let existing_raw = std::fs::read_to_string(&path).unwrap_or_default();

    let (partition_key, partition_data) = match agent_type {
        SubAgentType::Explore => {
            // Explorer: write findings from structured verdict (P1+)
            // CRAFT V2: also extract coverage metadata from the output
            let mut partition = json!({
                "findings": build_explorer_findings(result),
                "impact_summary": extract_impact_summary(result),
                "files_examined": extract_files_examined(result),
                "coverage_confidence": extract_coverage_confidence(result),
            });
            if let Some(findings) = &result.structured_findings
                && let Value::Object(ref mut map) = partition
            {
                map.insert(
                    "structured_findings".to_string(),
                    serde_json::to_value(findings).unwrap_or(Value::Null),
                );
            }
            ("explorer", partition)
        }
        SubAgentType::Implementer => {
            // CRAFT V2: append current round to rounds[] history
            (
                "implementer",
                build_implementer_rounds(result, &existing_raw, workspace),
            )
        }
        SubAgentType::Review => {
            // C4: track rounds[] like implementer so reviewer.rounds is populated.
            // Latest `verdict` / `blockers` / `structured_verdict` keys kept at
            // top-level for backwards-compatible readers.
            let verdict_val = result
                .structured_verdict
                .as_ref()
                .map(|v| serde_json::to_value(&v.verdict).unwrap_or(json!("PASS")))
                .unwrap_or(json!("PASS"));
            let blockers_val: Value = result
                .structured_verdict
                .as_ref()
                .map(|v| serde_json::to_value(&v.items).unwrap_or(json!([])))
                .unwrap_or(json!([]));
            let mut partition = json!({
                "verdict": verdict_val,
                "blockers": blockers_val,
            });
            if let Some(sv) = &result.structured_verdict
                && let Value::Object(ref mut map) = partition
            {
                map.insert(
                    "structured_verdict".to_string(),
                    serde_json::to_value(sv).unwrap_or(Value::Null),
                );
            }
            // Append round record
            if let Value::Object(ref mut map) = partition {
                let rounds = build_reviewer_rounds(result, &existing_raw);
                map.insert("rounds".to_string(), rounds);
            }
            ("reviewer", partition)
        }
        SubAgentType::Verifier => {
            // C4: track rounds[] for verifier symmetry.
            let verdict_val = result
                .structured_verdict
                .as_ref()
                .map(|v| serde_json::to_value(&v.verdict).unwrap_or(json!("PASS")))
                .unwrap_or(json!("PASS"));
            let rounds = build_verifier_rounds(result, &existing_raw);
            (
                "verifier",
                json!({
                    "verdict": verdict_val,
                    "failures": build_verifier_failures(result),
                    "summary": extract_verifier_summary(result),
                    "rounds": rounds,
                }),
            )
        }
        SubAgentType::Auditor => (
            "auditor",
            json!({
                "verdict": result.structured_verdict.as_ref()
                    .map(|v| serde_json::to_value(&v.verdict).unwrap_or(json!("FAIL")))
                    .unwrap_or(json!("FAIL")),
                "details": extract_auditor_details(result),
            }),
        ),
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
    blackboard_cache::invalidate(&path);
}

/// Read structured audit findings written by an Explore agent.
pub fn read_structured_findings_from_blackboard(
    workspace: &Path,
    task_id: &str,
    agent_type: &SubAgentType,
) -> Option<StructuredFindings> {
    if !matches!(agent_type, SubAgentType::Explore) {
        return None;
    }
    let board = read_blackboard_raw(workspace, task_id)?;
    board
        .get("explorer")?
        .get("structured_findings")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Read structured CRAFT verdict written by a Review agent.
pub fn read_structured_verdict_from_blackboard(
    workspace: &Path,
    task_id: &str,
    agent_type: &SubAgentType,
) -> Option<StructuredVerdict> {
    if !matches!(agent_type, SubAgentType::Review) {
        return None;
    }
    let board = read_blackboard_raw(workspace, task_id)?;
    board
        .get("reviewer")?
        .get("structured_verdict")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// BLOCKER-severity items from the latest CRAFT review blackboard partition.
#[must_use]
pub fn read_reviewer_blocker_items(
    workspace: &Path,
    task_id: &str,
) -> Vec<zagens_core::subagent::VerdictItem> {
    read_structured_verdict_from_blackboard(workspace, task_id, &SubAgentType::Review)
        .map(|v| v.items)
        .unwrap_or_default()
        .into_iter()
        .filter(|item| item.severity.eq_ignore_ascii_case("BLOCKER"))
        .collect()
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
        let suggestion = f
            .get("suggestion")
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
        let line = b
            .get("line")
            .and_then(|v| v.as_u64())
            .map(|l| format!(":{l}"))
            .unwrap_or_default();
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
        let observed = f.get("observed").and_then(|v| v.as_str()).unwrap_or("?");
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

/// Count completed Implementer rounds for a CRAFT task (fix-loop circuit breaker).
#[must_use]
pub fn implementer_round_count(workspace: &Path, task_id: &str) -> u32 {
    read_blackboard_raw(workspace, task_id)
        .and_then(|board| board.get("implementer").cloned())
        .and_then(|part| implementer_rounds_slice(&part).map(<[_]>::len))
        .map(|n| n as u32)
        .unwrap_or(0)
}

/// Count how many Reviewer rounds have been written to the blackboard.
pub fn reviewer_round_count(workspace: &Path, task_id: &str) -> u32 {
    read_blackboard_raw(workspace, task_id)
        .and_then(|board| board.get("reviewer").cloned())
        .and_then(|part| {
            part.get("rounds")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
        })
        .map(|n| n as u32)
        .unwrap_or(0)
}

/// Count how many Verifier rounds have been written to the blackboard.
pub fn verifier_round_count(workspace: &Path, task_id: &str) -> u32 {
    read_blackboard_raw(workspace, task_id)
        .and_then(|board| board.get("verifier").cloned())
        .and_then(|part| {
            part.get("rounds")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
        })
        .map(|n| n as u32)
        .unwrap_or(0)
}

/// Read implementer `rounds` from partition value (supports legacy bare-array writes).
fn implementer_rounds_slice(implementer: &Value) -> Option<&[Value]> {
    if let Some(rounds) = implementer.get("rounds").and_then(|v| v.as_array()) {
        return Some(rounds.as_slice());
    }
    implementer.as_array().map(|a| a.as_slice())
}

fn format_change_line(change: &Value) -> Option<String> {
    if let Some(file) = change.as_str().filter(|s| !s.is_empty()) {
        return Some(format!("- `{file}`"));
    }
    let file = change.get("file").and_then(|v| v.as_str()).unwrap_or("?");
    let intent = change
        .get("intent")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    Some(match intent {
        Some(intent) => format!("- `{file}` — {intent}"),
        None => format!("- `{file}`"),
    })
}

fn format_implementer_changes(board: &Value) -> Option<String> {
    let rounds = implementer_rounds_slice(board.get("implementer")?)?;
    if rounds.is_empty() {
        return None;
    }
    let mut lines = vec!["### Implementer changes".to_string()];
    for round in rounds {
        let changes = round.get("changes")?.as_array()?;
        for c in changes {
            if let Some(line) = format_change_line(c) {
                lines.push(line);
            }
        }
    }
    // Append factual symbol index change record (not model-narrated).
    if let Some(round) = rounds.last()
        && let Some(sc) = round.get("symbol_changes")
        && !sc.is_null()
    {
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
    Some(lines.join("\n"))
}

// ── Data extractors (write side) ───────────────────────────────

fn build_explorer_findings(result: &SubAgentResult) -> Value {
    if let Some(f) = &result.structured_findings {
        let items: Vec<Value> = f
            .items
            .iter()
            .map(|item| {
                json!({
                    "file": item.file,
                    "line": item.line,
                    "line_end": item.line_end,
                    "concern": item.claim,
                    "severity": item.severity,
                    "evidence": item.evidence,
                    "kind": item.kind,
                })
            })
            .collect();
        return json!(items);
    }
    // Fallback: CRAFT structured_verdict (legacy explore output)
    match &result.structured_verdict {
        Some(v) => {
            let items: Vec<Value> = v
                .items
                .iter()
                .map(|item| {
                    json!({
                        "file": item.file,
                        "concern": item.description,
                        "severity": item.severity,
                        "suggestion": item.suggestion,
                    })
                })
                .collect();
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
    if !matches!(
        v.verdict,
        super::VerdictLevel::Fail | super::VerdictLevel::Blocker
    ) {
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
        .unwrap_or(result.result.as_deref().unwrap_or(""))
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

// ── C11: requirements partition (traceability matrix) ───────────

/// A single requirement entry in the `requirements` blackboard partition.
///
/// Written by the main agent before CRAFT begins; checklist items can then
/// carry `[req: id]` tags to bind themselves back to a requirement.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RequirementEntry {
    /// Short stable ID (e.g. `"R1"`, `"AUTH-1"`, `"perf-budget"`).
    pub id: String,
    /// Human-readable description of the requirement.
    pub text: String,
}

/// Write (or overwrite) the `requirements` partition in the blackboard.
///
/// Call this once per CRAFT task, typically at the start of the Explorer
/// phase, to record the authoritative requirement list for the task.
/// Later, `check_requirement_coverage` / `format_requirements_for_reviewer`
/// use this list to compute traceability.
pub fn write_task_requirements(workspace: &Path, task_id: &str, requirements: &[RequirementEntry]) {
    if requirements.is_empty() {
        return;
    }
    let entries: Vec<Value> = requirements
        .iter()
        .map(|r| json!({ "id": r.id, "text": r.text }))
        .collect();
    merge_board_partition(workspace, task_id, "requirements", Value::Array(entries));
}

/// Read the `requirements` partition from the blackboard.
///
/// Returns an empty `Vec` when no requirements have been registered.
#[must_use]
pub fn read_task_requirements(workspace: &Path, task_id: &str) -> Vec<RequirementEntry> {
    let board = match read_blackboard_raw(workspace, task_id) {
        Some(b) => b,
        None => return Vec::new(),
    };
    board
        .get("requirements")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let id = e.get("id")?.as_str()?.trim().to_string();
                    let text = e.get("text")?.as_str()?.trim().to_string();
                    if id.is_empty() {
                        None
                    } else {
                        Some(RequirementEntry { id, text })
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A requirement entry paired with the checklist items that claim to implement it.
#[derive(Debug, Clone)]
pub struct RequirementCoverage {
    pub requirement: RequirementEntry,
    /// Content of checklist items carrying `[req: <id>]` for this requirement.
    pub covered_by: Vec<String>,
}

impl RequirementCoverage {
    /// `true` when at least one checklist item references this requirement.
    #[must_use]
    pub fn is_covered(&self) -> bool {
        !self.covered_by.is_empty()
    }
}

/// Compute coverage of registered requirements against a set of checklist items.
///
/// Each checklist item that carries a `[req: ID]` tag counts as evidence that
/// requirement `ID` is addressed. Requirements with zero covering items are
/// returned as uncovered — these are the actionable traceability gaps.
///
/// # Example
/// ```ignore
/// let coverage = check_requirement_coverage(workspace, task_id, &checklist.items);
/// let orphans: Vec<_> = coverage.iter().filter(|c| !c.is_covered()).collect();
/// ```
#[must_use]
pub fn check_requirement_coverage(
    workspace: &Path,
    task_id: &str,
    checklist_items: &[crate::tools::todo::TodoItem],
) -> Vec<RequirementCoverage> {
    let requirements = read_task_requirements(workspace, task_id);
    if requirements.is_empty() {
        return Vec::new();
    }

    requirements
        .into_iter()
        .map(|req| {
            let covered_by: Vec<String> = checklist_items
                .iter()
                .filter(|item| {
                    crate::long_horizon::parse_all_req_tags(&item.content)
                        .iter()
                        .any(|tag| tag == &req.id)
                })
                .map(|item| item.content.clone())
                .collect();
            RequirementCoverage {
                requirement: req,
                covered_by,
            }
        })
        .collect()
}

/// Format the requirements coverage report for injection into the Reviewer prompt.
///
/// When requirements have been registered for this task, the Reviewer receives
/// a compact table showing which requirements have checklist coverage and which
/// are orphaned (no `[req: ID]` tag on any checklist item).
///
/// Returns `None` when no requirements are registered (backwards-compatible — no
/// change to existing CRAFT workflows that don't use requirements).
#[must_use]
pub fn format_requirements_for_reviewer(
    workspace: &Path,
    task_id: &str,
    checklist_items: &[crate::tools::todo::TodoItem],
) -> Option<String> {
    let coverage = check_requirement_coverage(workspace, task_id, checklist_items);
    if coverage.is_empty() {
        return None;
    }

    let mut lines = vec!["### Requirements traceability".to_string()];
    let orphan_count = coverage.iter().filter(|c| !c.is_covered()).count();

    for cov in &coverage {
        if cov.is_covered() {
            lines.push(format!(
                "- ✅ `{}`: {} — covered by {} checklist item(s)",
                cov.requirement.id,
                cov.requirement.text,
                cov.covered_by.len()
            ));
        } else {
            lines.push(format!(
                "- ⚠️  `{}`: {} — **NO checklist item carries `[req: {}]`**",
                cov.requirement.id, cov.requirement.text, cov.requirement.id
            ));
        }
    }

    if orphan_count > 0 {
        lines.push(String::new());
        lines.push(format!(
            "> {orphan_count} requirement(s) above have no traceability link. \
             Add `[req: ID]` to the relevant checklist item(s) or raise a BLOCKER \
             if the requirement was never addressed."
        ));
    }

    Some(lines.join("\n"))
}

fn merge_board_partition(
    workspace: &Path,
    task_id: &str,
    partition_key: &str,
    partition_data: Value,
) {
    let Ok(path) = blackboard_path_write(workspace, task_id) else {
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
    blackboard_cache::invalidate(&path);
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
        std::env::temp_dir().join(format!("deepseek-blackboard-test-{}", std::process::id()))
    }

    #[test]
    fn test_blackboard_path_write_contains_task_id() {
        let ws = test_workspace();
        let path = blackboard_path_write(&ws, "bugfix-001").expect("valid task id");
        let s = path.to_string_lossy();
        assert!(
            s.contains("bugfix-001"),
            "path should contain task id, got: {s}"
        );
        assert!(s.ends_with(".json"), "path should end with .json, got: {s}");
        assert!(
            s.contains(&ws.to_string_lossy().to_string()),
            "path should be under workspace"
        );
    }

    #[test]
    fn test_blackboard_path_write_rejects_traversal() {
        let ws = test_workspace();
        assert!(blackboard_path_write(&ws, "/tmp/evil").is_err());
        assert!(blackboard_path_write(&ws, "..\\escape").is_err());
    }

    #[test]
    fn test_read_blackboard_returns_none_for_missing_file() {
        let ws = test_workspace();
        let result =
            read_blackboard_section(&ws, "nonexistent-task-99999", &SubAgentType::Implementer);
        assert!(result.is_none(), "missing file should return None");
    }

    #[test]
    fn test_list_and_read_blackboard_raw() {
        let ws = test_workspace();
        let task_id = "list-test-001";
        let path = blackboard_path_write(&ws, task_id).expect("valid task id");
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
        StructuredVerdict, SubAgentAssignment, SubAgentResult, SubAgentStatus, SubAgentType as SAT,
        VerdictItem, VerdictLevel,
    };

    #[test]
    fn test_write_and_read_explorer_findings() {
        let ws = test_workspace();
        let task_id = "test-001";
        let _ = std::fs::remove_file(blackboard_path_write(&ws, task_id).expect("valid task id"));

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
            structured_findings: None,
            completion_reason: None,
            max_steps: 100,
            step_timeout_ms: 600_000,
            structured_findings_parse_failure: None,
            scratchpad_run_id: None,
            parent_thread_id: None,
            progress_status: None,
            stuck_suspected: false,
            idle_ms: 0,
        };

        // Write to blackboard
        write_blackboard_partition(&ws, task_id, &SAT::Explore, &result);

        // Read back as Implementer
        let section = read_blackboard_section(&ws, task_id, &SAT::Implementer)
            .expect("should read back explorer findings for implementer");

        assert!(
            section.contains("### Explorer findings"),
            "section: {section}"
        );
        assert!(section.contains("auth/login.rs"), "section: {section}");
        assert!(
            section.contains("token uses standard RNG"),
            "section: {section}"
        );
        assert!(section.contains("replace with OsRng"), "section: {section}");
        assert!(section.contains("auth/session.rs"), "section: {section}");
        assert!(section.contains("session timeout"), "section: {section}");

        // Clean up
        let _ = std::fs::remove_file(blackboard_path_write(&ws, task_id).expect("valid task id"));
    }

    #[test]
    fn test_write_and_read_roundtrip_multiple_roles() {
        let ws = test_workspace();
        let task_id = "test-002";
        let _ = std::fs::remove_file(blackboard_path_write(&ws, task_id).expect("valid task id"));

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
            structured_findings: None,
            completion_reason: None,
            max_steps: 100,
            step_timeout_ms: 600_000,
            structured_findings_parse_failure: None,
            scratchpad_run_id: None,
            parent_thread_id: None,
            progress_status: None,
            stuck_suspected: false,
            idle_ms: 0,
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
            structured_findings: None,
            completion_reason: None,
            max_steps: 100,
            step_timeout_ms: 600_000,
            structured_findings_parse_failure: None,
            scratchpad_run_id: None,
            parent_thread_id: None,
            progress_status: None,
            stuck_suspected: false,
            idle_ms: 0,
        };
        write_blackboard_partition(&ws, task_id, &SAT::Review, &reviewer_result);

        // Read back as Implementer — should see BOTH
        let section = read_blackboard_section(&ws, task_id, &SAT::Implementer)
            .expect("should read both sections");

        assert!(
            section.contains("### Explorer findings"),
            "section: {section}"
        );
        assert!(section.contains("unsafe usage"), "section: {section}");
        assert!(
            section.contains("### Reviewer blockers"),
            "section: {section}"
        );
        assert!(section.contains("missing null check"), "section: {section}");

        let _ = std::fs::remove_file(blackboard_path_write(&ws, task_id).expect("valid task id"));
    }

    #[test]
    fn test_verifier_failures_injected_for_implementer() {
        let ws = test_workspace();
        let task_id = "test-verifier-003";
        let _ = std::fs::remove_file(blackboard_path_write(&ws, task_id).expect("valid task id"));

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
            structured_findings: None,
            completion_reason: None,
            max_steps: 100,
            step_timeout_ms: 600_000,
            structured_findings_parse_failure: None,
            scratchpad_run_id: None,
            parent_thread_id: None,
            progress_status: None,
            stuck_suspected: false,
            idle_ms: 0,
        };
        write_blackboard_partition(&ws, task_id, &SAT::Verifier, &verifier_result);

        let section = read_blackboard_section(&ws, task_id, &SAT::Implementer)
            .expect("implementer should read verifier failures");
        assert!(section.contains("### Verifier failures"), "{section}");
        assert!(section.contains("assertion failed"), "{section}");
        assert!(section.contains("fix test setup"), "{section}");

        let _ = std::fs::remove_file(blackboard_path_write(&ws, task_id).expect("valid task id"));
    }

    #[test]
    fn test_implementer_rounds_injected_for_review() {
        let ws = test_workspace();
        let task_id = "test-impl-rounds";
        let _ = std::fs::remove_file(blackboard_path_write(&ws, task_id).expect("valid task id"));

        let implementer_result = SubAgentResult {
            agent_id: "i1".into(),
            agent_type: SAT::Implementer,
            assignment: SubAgentAssignment::new("fix".into(), None),
            model: "deepseek-v4-flash".into(),
            nickname: None,
            status: SubAgentStatus::Completed,
            result: Some("CHANGES\nModified: crates/foo/src/lib.rs — add null check\n".into()),
            steps_taken: 1,
            duration_ms: 100,
            from_prior_session: false,
            structured_verdict: None,
            structured_findings: None,
            completion_reason: None,
            max_steps: 100,
            step_timeout_ms: 600_000,
            structured_findings_parse_failure: None,
            scratchpad_run_id: None,
            parent_thread_id: None,
            progress_status: None,
            stuck_suspected: false,
            idle_ms: 0,
        };
        write_blackboard_partition(&ws, task_id, &SAT::Implementer, &implementer_result);

        let raw = read_blackboard_raw(&ws, task_id).expect("board");
        let rounds = raw["implementer"]["rounds"]
            .as_array()
            .expect("implementer.rounds array");
        assert_eq!(rounds.len(), 1);
        assert_eq!(rounds[0]["round"], 1);
        let changes = rounds[0]["changes"].as_array().expect("changes");
        assert_eq!(changes[0]["file"], "crates/foo/src/lib.rs");
        assert_eq!(changes[0]["intent"], "add null check");

        let section = read_blackboard_section(&ws, task_id, &SAT::Review)
            .expect("reviewer should see implementer changes");
        assert!(section.contains("### Implementer changes"), "{section}");
        assert!(section.contains("crates/foo/src/lib.rs"), "{section}");
        assert!(section.contains("add null check"), "{section}");
        assert_eq!(implementer_round_count(&ws, task_id), 1);

        let _ = std::fs::remove_file(blackboard_path_write(&ws, task_id).expect("valid task id"));
    }

    #[test]
    fn test_reviewer_and_verifier_round_count() {
        use zagens_core::subagent::{StructuredVerdict, VerdictLevel};

        let ws = test_workspace();
        let task_id = "test-reviewer-rounds";
        let _ = std::fs::remove_file(blackboard_path_write(&ws, task_id).expect("valid task id"));

        assert_eq!(reviewer_round_count(&ws, task_id), 0, "no board yet");
        assert_eq!(verifier_round_count(&ws, task_id), 0, "no board yet");

        let review_result = SubAgentResult {
            agent_id: "r1".into(),
            agent_type: SAT::Review,
            assignment: SubAgentAssignment::new("review".into(), None),
            model: "deepseek-v4-flash".into(),
            nickname: None,
            status: SubAgentStatus::Completed,
            result: Some("VERDICT: PASS".into()),
            steps_taken: 1,
            duration_ms: 100,
            from_prior_session: false,
            structured_verdict: Some(StructuredVerdict {
                verdict: VerdictLevel::Pass,
                items: vec![],
                summary: Some("looks good".into()),
            }),
            structured_findings: None,
            completion_reason: None,
            max_steps: 100,
            step_timeout_ms: 600_000,
            structured_findings_parse_failure: None,
            scratchpad_run_id: None,
            parent_thread_id: None,
            progress_status: None,
            stuck_suspected: false,
            idle_ms: 0,
        };
        write_blackboard_partition(&ws, task_id, &SAT::Review, &review_result);
        assert_eq!(reviewer_round_count(&ws, task_id), 1, "after first review");

        // Second reviewer round
        write_blackboard_partition(&ws, task_id, &SAT::Review, &review_result);
        assert_eq!(reviewer_round_count(&ws, task_id), 2, "after second review");

        let verifier_result = SubAgentResult {
            agent_id: "v1".into(),
            agent_type: SAT::Verifier,
            assignment: SubAgentAssignment::new("verify".into(), None),
            model: "deepseek-v4-flash".into(),
            nickname: None,
            status: SubAgentStatus::Completed,
            result: Some("VERDICT: PASS".into()),
            steps_taken: 1,
            duration_ms: 100,
            from_prior_session: false,
            structured_verdict: Some(StructuredVerdict {
                verdict: VerdictLevel::Pass,
                items: vec![],
                summary: None,
            }),
            structured_findings: None,
            completion_reason: None,
            max_steps: 100,
            step_timeout_ms: 600_000,
            structured_findings_parse_failure: None,
            scratchpad_run_id: None,
            parent_thread_id: None,
            progress_status: None,
            stuck_suspected: false,
            idle_ms: 0,
        };
        write_blackboard_partition(&ws, task_id, &SAT::Verifier, &verifier_result);
        assert_eq!(verifier_round_count(&ws, task_id), 1, "after first verify");
        assert_eq!(
            reviewer_round_count(&ws, task_id),
            2,
            "reviewer count unchanged"
        );

        // Verify round data written
        let raw = read_blackboard_raw(&ws, task_id).expect("board");
        assert_eq!(raw["reviewer"]["rounds"].as_array().unwrap().len(), 2);
        assert_eq!(raw["reviewer"]["rounds"][0]["round"], 1);
        assert_eq!(raw["reviewer"]["rounds"][1]["round"], 2);
        assert_eq!(raw["verifier"]["rounds"][0]["round"], 1);

        let _ = std::fs::remove_file(blackboard_path_write(&ws, task_id).expect("valid task id"));
    }
} // end mod tests

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
            if trimmed.starts_with("- **Files examined**")
                || trimmed.starts_with("- Files examined")
            {
                in_files = true;
                continue;
            }
            if in_files {
                if trimmed.starts_with("- ") && !trimmed.starts_with("- **") {
                    let path = trimmed.trim_start_matches("- ").trim();
                    if !path.is_empty() {
                        files.push(path.to_string());
                    }
                } else if trimmed.starts_with("- **")
                    || trimmed.starts_with("##")
                    || trimmed.is_empty()
                {
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
    if let Some(re) = re
        && let Some(cap) = re.captures(text)
        && let Some(m) = cap.get(1)
    {
        return json!(m.as_str());
    }
    json!("unknown")
}

fn load_existing_implementer_rounds(existing_raw: &str) -> Vec<Value> {
    if existing_raw.trim().is_empty() {
        return Vec::new();
    }
    let Ok(board) = serde_json::from_str::<Value>(existing_raw) else {
        return Vec::new();
    };
    let Some(implementer) = board.get("implementer") else {
        return Vec::new();
    };
    implementer_rounds_slice(implementer)
        .map(<[Value]>::to_vec)
        .unwrap_or_default()
}

fn read_last_reviewer_verdict(existing_raw: &str) -> Value {
    serde_json::from_str::<Value>(existing_raw)
        .ok()
        .and_then(|board| board.get("reviewer").cloned())
        .and_then(|reviewer| reviewer.get("verdict").cloned())
        .unwrap_or(Value::Null)
}

fn build_implementer_rounds(
    result: &SubAgentResult,
    existing_raw: &str,
    workspace: &Path,
) -> Value {
    let mut existing_rounds = load_existing_implementer_rounds(existing_raw);
    let round_num = existing_rounds.len() + 1;
    let changes = collect_implementer_changes(workspace, result);
    let symbol_changes = read_symbol_changes(workspace);
    let reviewer_verdict = read_last_reviewer_verdict(existing_raw);

    let new_round = json!({
        "round": round_num,
        "changes": changes,
        "symbol_changes": symbol_changes,
        "reviewer_verdict": reviewer_verdict,
    });

    existing_rounds.push(new_round);
    json!({ "rounds": existing_rounds })
}

/// Accumulate reviewer `rounds[]`. Each round captures the verdict and
/// which implementer round the reviewer was evaluating.
fn build_reviewer_rounds(result: &SubAgentResult, existing_raw: &str) -> Value {
    let mut existing: Vec<Value> = serde_json::from_str::<Value>(existing_raw)
        .ok()
        .and_then(|b| b.get("reviewer").and_then(|r| r.get("rounds")).cloned())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    let round_num = existing.len() + 1;
    // Which implementer round is this review evaluating?
    let impl_round = serde_json::from_str::<Value>(existing_raw)
        .ok()
        .and_then(|b| b.get("implementer").cloned())
        .and_then(|p| implementer_rounds_slice(&p).map(|s| s.len() as u32))
        .unwrap_or(0);

    let verdict_str = result
        .structured_verdict
        .as_ref()
        .map(|v| super::craft::verdict_level_str(&v.verdict))
        .unwrap_or("PASS");
    let summary = result
        .structured_verdict
        .as_ref()
        .and_then(|v| v.summary.as_deref())
        .unwrap_or("");
    let blockers_count = result
        .structured_verdict
        .as_ref()
        .map(|v| v.items.len())
        .unwrap_or(0);

    existing.push(json!({
        "round": round_num,
        "verdict": verdict_str,
        "blockers_count": blockers_count,
        "summary": summary,
        "evaluated_implementer_round": impl_round,
    }));
    Value::Array(existing)
}

/// Accumulate verifier `rounds[]`. Symmetric with reviewer rounds.
fn build_verifier_rounds(result: &SubAgentResult, existing_raw: &str) -> Value {
    let mut existing: Vec<Value> = serde_json::from_str::<Value>(existing_raw)
        .ok()
        .and_then(|b| b.get("verifier").and_then(|r| r.get("rounds")).cloned())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    let round_num = existing.len() + 1;
    let impl_round = serde_json::from_str::<Value>(existing_raw)
        .ok()
        .and_then(|b| b.get("implementer").cloned())
        .and_then(|p| implementer_rounds_slice(&p).map(|s| s.len() as u32))
        .unwrap_or(0);

    let verdict_str = result
        .structured_verdict
        .as_ref()
        .map(|v| super::craft::verdict_level_str(&v.verdict))
        .unwrap_or("PASS");
    let failures_count = build_verifier_failures(result)
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    existing.push(json!({
        "round": round_num,
        "verdict": verdict_str,
        "failures_count": failures_count,
        "evaluated_implementer_round": impl_round,
    }));
    Value::Array(existing)
}

fn git_diff_name_only(workspace: &Path) -> Vec<String> {
    let Ok(output) = Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .current_dir(workspace)
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn collect_implementer_changes(workspace: &Path, result: &SubAgentResult) -> Value {
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut seen = HashSet::new();

    for path in git_diff_name_only(workspace) {
        if seen.insert(path.clone()) {
            entries.push((path, "git diff".into()));
        }
    }

    for (path, intent) in parse_changes_from_result_text(result) {
        if seen.insert(path.clone()) {
            entries.push((path, intent));
        } else if let Some(entry) = entries.iter_mut().find(|(p, _)| *p == path)
            && entry.1 == "git diff"
            && intent != "git diff"
        {
            entry.1 = intent;
        }
    }

    let items: Vec<Value> = entries
        .into_iter()
        .map(|(file, intent)| json!({ "file": file, "intent": intent }))
        .collect();
    json!(items)
}

fn read_symbol_changes(workspace: &Path) -> Value {
    let path = workspace_meta_file_read(workspace, ".symbols_changes.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or(json!(null))
}

fn parse_changes_from_result_text(result: &SubAgentResult) -> Vec<(String, String)> {
    let text = result.result.as_deref().unwrap_or("");
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    if let Ok(re) = regex::Regex::new(
        r"(?m)^\s*(?:Modified|Changed|Added|Edited):\s*(.+?)(?:\s*[—–-]\s*(.+))?\s*$",
    ) {
        for cap in re.captures_iter(text) {
            let path = cap.get(1).map(|m| m.as_str().trim().to_string());
            let intent = cap
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "reported in agent output".into());
            if let Some(path) = path.filter(|p| !p.is_empty())
                && seen.insert(path.clone())
            {
                out.push((path, intent));
            }
        }
    }

    if out.is_empty()
        && let Ok(path_re) =
            regex::Regex::new(r"`((?:crates|src|web-ui)/\S+\.(?:rs|toml|ts|tsx|js|json|md))`")
    {
        for cap in path_re.captures_iter(text) {
            if let Some(m) = cap.get(1) {
                let path = m.as_str().to_string();
                if seen.insert(path.clone()) {
                    out.push((path, "reported in agent output".into()));
                }
            }
        }
    }

    out
}

// ── C11 requirement traceability tests ─────────────────────────

#[cfg(test)]
mod requirement_tests {
    use super::*;
    use crate::tools::todo::{TodoItem, TodoStatus};

    fn test_workspace() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("deepseek-req-test-{}", std::process::id()))
    }

    fn make_item(id: u32, content: &str) -> TodoItem {
        TodoItem {
            id,
            content: content.to_string(),
            status: TodoStatus::Completed,
        }
    }

    #[test]
    fn write_and_read_requirements() {
        let ws = test_workspace();
        let task_id = "req-test-001";
        let reqs = vec![
            RequirementEntry {
                id: "R1".to_string(),
                text: "must handle auth".to_string(),
            },
            RequirementEntry {
                id: "R2".to_string(),
                text: "must return 404 on missing resource".to_string(),
            },
        ];
        write_task_requirements(&ws, task_id, &reqs);

        let read_back = read_task_requirements(&ws, task_id);
        assert_eq!(read_back.len(), 2);
        assert_eq!(read_back[0].id, "R1");
        assert_eq!(read_back[1].text, "must return 404 on missing resource");
        let _ = std::fs::remove_file(blackboard_path_write(&ws, task_id).expect("valid id"));
    }

    #[test]
    fn coverage_check_identifies_orphaned() {
        let ws = test_workspace();
        let task_id = "req-test-002";
        let reqs = vec![
            RequirementEntry {
                id: "R1".to_string(),
                text: "auth".to_string(),
            },
            RequirementEntry {
                id: "R2".to_string(),
                text: "404 handling".to_string(),
            },
        ];
        write_task_requirements(&ws, task_id, &reqs);

        let items = vec![
            make_item(1, "[verify: cargo test] [req: R1] auth tests pass"),
            make_item(2, "implement router"),
        ];
        let coverage = check_requirement_coverage(&ws, task_id, &items);
        assert_eq!(coverage.len(), 2);

        let r1 = coverage.iter().find(|c| c.requirement.id == "R1").unwrap();
        assert!(r1.is_covered(), "R1 should be covered");

        let r2 = coverage.iter().find(|c| c.requirement.id == "R2").unwrap();
        assert!(!r2.is_covered(), "R2 should be orphaned");

        let _ = std::fs::remove_file(blackboard_path_write(&ws, task_id).expect("valid id"));
    }

    #[test]
    fn format_reviewer_shows_orphan_warning() {
        let ws = test_workspace();
        let task_id = "req-test-003";
        let reqs = vec![
            RequirementEntry {
                id: "R1".to_string(),
                text: "auth".to_string(),
            },
            RequirementEntry {
                id: "R2".to_string(),
                text: "404".to_string(),
            },
        ];
        write_task_requirements(&ws, task_id, &reqs);

        let items = vec![make_item(1, "[req: R1] implement auth handler")];
        let section =
            format_requirements_for_reviewer(&ws, task_id, &items).expect("should produce section");
        assert!(section.contains("✅"), "covered R1 should show checkmark");
        assert!(section.contains("⚠️"), "orphaned R2 should show warning");
        assert!(section.contains("R2"), "should mention R2");

        let _ = std::fs::remove_file(blackboard_path_write(&ws, task_id).expect("valid id"));
    }

    #[test]
    fn empty_requirements_returns_none_for_reviewer() {
        let ws = test_workspace();
        let task_id = "req-test-004-empty";
        // No requirements written at all
        let section = format_requirements_for_reviewer(&ws, task_id, &[]);
        assert!(section.is_none(), "should return None when no requirements");
    }
}
