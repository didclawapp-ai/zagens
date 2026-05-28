//! Import structured sub-agent findings into scratchpad notes.

use deepseek_core::subagent::{AuditFindingItem, StructuredFindings, StructuredVerdict, SubAgentResult};
use serde_json::json;

use crate::tools::spec::ToolError;

use super::ScratchpadStore;
use super::schema::NoteLine;
use super::{compute_superseded_ids, is_high_severity, is_open_finding};

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

    let source = format!("agent:{}", result.agent_id);
    let mut imported = Vec::new();

    if let Some(findings) = &result.structured_findings {
        imported.extend(import_structured_findings(store, findings, &source)?);
        return Ok(imported);
    }

    if let Some(verdict) = &result.structured_verdict {
        let area_id = area_id_override
            .map(str::to_string)
            .unwrap_or_else(|| "_global".to_string());
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
        "agent '{}' has no structured_findings or structured_verdict; re-run explorer with <!-- audit-findings --> output",
        result.agent_id
    )))
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
