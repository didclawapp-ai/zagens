//! Import structured sub-agent findings into scratchpad notes.

use deepseek_core::subagent::{AuditFindingItem, CompletionReason, ParseFailureReason, StructuredFindings, StructuredVerdict, SubAgentResult};
use serde_json::json;

use crate::tools::spec::ToolError;

use super::ScratchpadStore;
use super::schema::NoteLine;
use super::{compute_superseded_ids, is_high_severity, is_open_finding};

/// Reject importing an agent into a scratchpad run it was not spawned for.
pub fn validate_agent_run_binding(
    agent_scratchpad_run_id: Option<&str>,
    target_run_id: &str,
    agent_id: &str,
) -> Result<(), ToolError> {
    if let Some(bound) = agent_scratchpad_run_id {
        if bound != target_run_id {
            return Err(ToolError::invalid_input(format!(
                "agent '{agent_id}' was spawned for scratchpad run '{bound}'; cannot import into '{target_run_id}'"
            )));
        }
    }
    Ok(())
}

fn validate_area_id_in_inventory(store: &ScratchpadStore, area_id: &str) -> Result<(), ToolError> {
    let area_id = area_id.trim();
    if area_id.is_empty() || area_id == "_global" {
        return Ok(());
    }
    let inventory = store.read_inventory()?;
    if !inventory.areas.iter().any(|a| a.id == area_id) {
        return Err(ToolError::invalid_input(format!(
            "area_id '{area_id}' is not in scratchpad run '{}' inventory; agent output may belong to a different audit run",
            store.run_id()
        )));
    }
    Ok(())
}

fn normalize_area_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim()
        .trim_end_matches('/')
        .to_string()
}

/// Resolve inventory row for import: explicit override → agent area_id → area_path match.
fn resolve_import_area_id(
    store: &ScratchpadStore,
    findings: &StructuredFindings,
    area_id_override: Option<&str>,
) -> Result<String, ToolError> {
    if let Some(override_id) = area_id_override.map(str::trim).filter(|s| !s.is_empty()) {
        validate_area_id_in_inventory(store, override_id)?;
        return Ok(override_id.to_string());
    }

    let agent_area_id = findings.area_id.trim();
    if !agent_area_id.is_empty() && validate_area_id_in_inventory(store, agent_area_id).is_ok() {
        return Ok(agent_area_id.to_string());
    }

    if let Some(path) = findings
        .area_path
        .as_deref()
        .map(normalize_area_path)
        .filter(|p| !p.is_empty())
    {
        let inventory = store.read_inventory()?;
        if let Some(area) = inventory
            .areas
            .iter()
            .find(|a| normalize_area_path(&a.path) == path)
        {
            return Ok(area.id.clone());
        }
    }

    Err(ToolError::invalid_input(format!(
        "area_id '{}' is not in scratchpad run '{}' inventory; pass scratchpad_import_agent area_id override or ensure structured_findings.area_id / area_path match inventory",
        if agent_area_id.is_empty() { "(empty)" } else { agent_area_id },
        store.run_id()
    )))
}

/// Import `structured_findings` (preferred) or `structured_verdict` from a completed sub-agent.
pub fn import_agent_findings(
    store: &ScratchpadStore,
    result: &SubAgentResult,
    area_id_override: Option<&str>,
) -> Result<Vec<NoteLine>, ToolError> {
    if !matches!(
        result.status,
        deepseek_core::subagent::SubAgentStatus::Completed
    ) {
        return Err(ToolError::invalid_input(format!(
            "agent '{}' status is {:?}; import only after completion",
            result.agent_id, result.status
        )));
    }

    if matches!(
        result.completion_reason,
        Some(CompletionReason::StepLimitReached)
            | Some(CompletionReason::StepApiTimeout)
            | Some(CompletionReason::Cancelled)
    ) {
        return Err(ToolError::invalid_input(format!(
            "agent '{}' completion_reason is {:?}; import only after NaturalBreak (re-spawn or narrow scope)",
            result.agent_id, result.completion_reason
        )));
    }

    let source = format!("agent:{}", result.agent_id);
    let mut imported = Vec::new();

    if let Some(findings) = &result.structured_findings {
        let area_id = resolve_import_area_id(store, findings, area_id_override)?;
        let mut remapped = findings.clone();
        remapped.area_id = area_id;
        imported.extend(import_structured_findings(store, &remapped, &source)?);
        return Ok(imported);
    }

    if let Some(verdict) = &result.structured_verdict {
        let area_id = area_id_override
            .map(str::to_string)
            .unwrap_or_else(|| "_global".to_string());
        validate_area_id_in_inventory(store, &area_id)?;
        imported.extend(import_verdict_as_findings(
            store,
            &area_id,
            None,
            verdict,
            &source,
        )?);
        return Ok(imported);
    }

    Err(ToolError::invalid_input(format!(
        "agent '{}' has no structured_findings or structured_verdict; re-run explorer with <!-- audit-findings --> output{}",
        result.agent_id,
        structured_import_hint(result)
    )))
}

fn structured_import_hint(result: &SubAgentResult) -> String {
    match result.structured_findings_parse_failure {
        Some(ParseFailureReason::Truncated) => {
            "; <!-- audit-findings --> was truncated — retry agent_result(block) or import with area_id override; salvage may recover partial items"
                .to_string()
        }
        Some(ParseFailureReason::NoMarker) => {
            "; final output missing <!-- audit-findings --> JSON fence".to_string()
        }
        Some(ParseFailureReason::InvalidJson(ref e)) => format!("; invalid JSON: {e}"),
        None => String::new(),
    }
}

fn import_structured_findings(
    store: &ScratchpadStore,
    findings: &StructuredFindings,
    source: &str,
) -> Result<Vec<NoteLine>, ToolError> {
    let area_id = findings.area_id.trim();
    if area_id.is_empty() {
        return Err(ToolError::invalid_input(
            "structured_findings.area_id must not be empty",
        ));
    }
    let mut out = Vec::new();
    for item in &findings.items {
        out.push(append_open_finding(
            store,
            area_id,
            findings.area_path.as_deref(),
            item,
            source,
        )?);
    }
    if findings.items.is_empty() {
        out.push(store.append_note(json!({
            "area_id": area_id,
            "kind": "cleared",
            "claim": findings.summary.clone().unwrap_or_else(|| "No findings".to_string()),
            "status": "open",
            "source": source,
        }))?);
    }
    Ok(out)
}

fn import_verdict_as_findings(
    store: &ScratchpadStore,
    area_id: &str,
    area_path: Option<&str>,
    verdict: &StructuredVerdict,
    source: &str,
) -> Result<Vec<NoteLine>, ToolError> {
    let mut out = Vec::new();
    for item in &verdict.items {
        let audit_item = AuditFindingItem {
            kind: "finding".to_string(),
            severity: item.severity.clone(),
            file: Some(item.file.clone()),
            line: item.line,
            line_end: None,
            claim: item.description.clone(),
            evidence: item.suggestion.clone(),
        };
        out.push(append_open_finding(
            store,
            area_id,
            area_path,
            &audit_item,
            source,
        )?);
    }
    Ok(out)
}

fn append_open_finding(
    store: &ScratchpadStore,
    area_id: &str,
    area_path: Option<&str>,
    item: &AuditFindingItem,
    source: &str,
) -> Result<NoteLine, ToolError> {
    let kind = if item.kind.eq_ignore_ascii_case("cleared") {
        "cleared"
    } else if item.kind.eq_ignore_ascii_case("meta") {
        "meta"
    } else {
        "finding"
    };
    let mut line = json!({
        "area_id": area_id,
        "kind": kind,
        "claim": item.claim,
        "status": "open",
        "source": source,
    });
    if let Some(path) = area_path {
        line["area"] = json!(path);
    }
    if kind == "finding" {
        if !item.severity.trim().is_empty() {
            line["severity"] = json!(item.severity.to_uppercase());
        }
        if let Some(ref f) = item.file {
            line["file"] = json!(f);
        }
        if let Some(l) = item.line {
            line["line"] = json!(l);
        }
        if let Some(l) = item.line_end {
            line["line_end"] = json!(l);
        }
        if let Some(ref e) = item.evidence {
            line["evidence"] = json!(e);
        }
    }
    store.append_note(line)
}

/// Promote an open note to `verified` by appending a superseding verified row (append-only).
pub fn verify_note(store: &ScratchpadStore, note_id: &str) -> Result<NoteLine, ToolError> {
    let notes = store.read_notes()?;
    let superseded = compute_superseded_ids(&notes);
    let original = notes
        .iter()
        .find(|n| n.id == note_id && !superseded.contains(note_id))
        .ok_or_else(|| ToolError::invalid_input(format!("note id '{note_id}' not found")))?;

    if original.status.eq_ignore_ascii_case("verified") {
        return Err(ToolError::invalid_input(format!(
            "note '{note_id}' is already verified"
        )));
    }

    let mut line = json!({
        "area_id": original.area_id,
        "kind": original.kind,
        "claim": original.claim.clone().unwrap_or_default(),
        "status": "verified",
        "source": "main",
        "supersedes": note_id,
    });
    if let Some(area) = &original.area {
        line["area"] = json!(area);
    }
    if let Some(sev) = &original.severity {
        line["severity"] = json!(sev);
    }
    if let Some(f) = &original.file {
        line["file"] = json!(f);
    }
    if let Some(l) = original.line {
        line["line"] = json!(l);
    }
    if let Some(l) = original.line_end {
        line["line_end"] = json!(l);
    }
    if let Some(e) = &original.evidence {
        line["evidence"] = json!(e);
    }
    store.append_note(line)
}

/// Returns open HIGH/BLOCKER finding ids for an area (post-supersedes).
pub fn open_high_finding_ids(store: &ScratchpadStore, area_id: &str) -> Result<Vec<String>, ToolError> {
    let notes = store.read_notes()?;
    let superseded = compute_superseded_ids(&notes);
    Ok(notes
        .iter()
        .filter(|n| {
            n.area_id == area_id
                && is_open_finding(n, &superseded)
                && is_high_severity(n.severity.as_deref())
        })
        .map(|n| n.id.clone())
        .collect())
}

#[cfg(test)]
mod import_gate_tests {
    use super::*;
    use crate::scratchpad::{ScratchpadStore, default_init_areas};
    use crate::tools::spec::ToolContext;
    use deepseek_core::subagent::{
        CompletionReason, StructuredFindings, SubAgentAssignment, SubAgentResult, SubAgentStatus,
        SubAgentType,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_store() -> (tempfile::TempDir, ScratchpadStore) {
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join(format!("ws-{n}"));
        std::fs::create_dir_all(&ws).expect("mkdir ws");
        let mut ctx = ToolContext::new(ws);
        ctx.runtime.wire.active_thread_id = Some(format!("thr-{n}"));
        let run_id = format!("thr-{n}");
        let store = ScratchpadStore::init(&ctx, &run_id, default_init_areas(), None)
            .expect("init scratchpad");
        (dir, store)
    }

    fn completed_result(reason: Option<CompletionReason>) -> SubAgentResult {
        SubAgentResult {
            agent_id: "agent_gate".into(),
            agent_type: SubAgentType::Explore,
            assignment: SubAgentAssignment::new("task".into(), None),
            model: "deepseek-v4-flash".into(),
            nickname: None,
            status: SubAgentStatus::Completed,
            result: Some("done".into()),
            steps_taken: 1,
            duration_ms: 100,
            from_prior_session: false,
            structured_findings: Some(StructuredFindings {
                area_id: "workspace".into(),
                area_path: None,
                items: vec![],
                summary: Some("no findings".into()),
            }),
            structured_verdict: None,
            completion_reason: reason,
            max_steps: 100,
            step_timeout_ms: 600_000,
            structured_findings_parse_failure: None,
            scratchpad_run_id: None,
            progress_status: None,
            stuck_suspected: false,
            idle_ms: 0,
        }
    }

    #[test]
    fn import_rejects_step_limit_completion_reason() {
        let (_dir, store) = temp_store();
        let result = completed_result(Some(CompletionReason::StepLimitReached));
        let err = import_agent_findings(&store, &result, None).expect_err("gate");
        assert!(err.to_string().contains("StepLimitReached"));
    }

    #[test]
    fn import_allows_natural_break_completion_reason() {
        let (_dir, store) = temp_store();
        let result = completed_result(Some(CompletionReason::NaturalBreak));
        let notes = import_agent_findings(&store, &result, None).expect("import ok");
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn validate_agent_run_binding_rejects_mismatched_run() {
        let err = validate_agent_run_binding(Some("run-a"), "run-b", "agent_x")
            .expect_err("mismatch");
        assert!(err.to_string().contains("run-a"));
        assert!(err.to_string().contains("run-b"));
    }

    #[test]
    fn import_rejects_area_id_not_in_target_run_inventory() {
        let (_dir, store) = temp_store();
        let mut result = completed_result(Some(CompletionReason::NaturalBreak));
        if let Some(ref mut findings) = result.structured_findings {
            findings.area_id = "stale-area-from-old-run".into();
        }
        let err = import_agent_findings(&store, &result, None).expect_err("bad area");
        assert!(err.to_string().contains("stale-area-from-old-run"));
    }

    #[test]
    fn import_honors_area_id_override_for_structured_findings() {
        let (_dir, store) = temp_store();
        let mut result = completed_result(Some(CompletionReason::NaturalBreak));
        if let Some(ref mut findings) = result.structured_findings {
            findings.area_id = "area-core-engine-part1".into();
            findings.area_path = Some("crates/core/src/engine".into());
        }
        let notes =
            import_agent_findings(&store, &result, Some("workspace")).expect("override import");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].area_id, "workspace");
    }

    #[test]
    fn import_remapped_by_area_path_when_agent_area_id_wrong() {
        let (_dir, store) = temp_store();
        let mut result = completed_result(Some(CompletionReason::NaturalBreak));
        if let Some(ref mut findings) = result.structured_findings {
            findings.area_id = "area-core-engine-part1".into();
            findings.area_path = Some(".".into());
        }
        let notes = import_agent_findings(&store, &result, None).expect("path remap");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].area_id, "workspace");
    }
}
