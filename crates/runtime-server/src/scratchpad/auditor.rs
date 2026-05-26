//! Phase C2: structured Auditor input from scratchpad verified findings.

use std::path::Path;

use crate::scratchpad::config::ScratchpadConfig;
use crate::scratchpad::schema::{NoteLine, is_high_severity, is_verified_finding};
use crate::scratchpad::summary::compute_superseded_ids;
use crate::scratchpad::{compute_coverage_stats, display_run_path, try_open_store};

/// Findings selected for Auditor track A (mechanical `read_file` by `note_id`).
#[derive(Debug, Clone)]
pub struct AuditorFindingSelection {
    pub track_a: Vec<NoteLine>,
    pub medium_skipped: usize,
    pub medium_included: bool,
}

/// Select verified findings for Auditor track A per §6.12.5.
#[must_use]
pub fn select_auditor_findings(
    notes: &[NoteLine],
    config: &ScratchpadConfig,
) -> AuditorFindingSelection {
    let superseded = compute_superseded_ids(notes);
    let mut highs = Vec::new();
    let mut mediums = Vec::new();

    for note in notes {
        if !is_verified_finding(note, &superseded) {
            continue;
        }
        let sev = note.severity.as_deref().unwrap_or("");
        if is_high_severity(Some(sev)) {
            highs.push(note.clone());
        } else if sev.eq_ignore_ascii_case("MEDIUM") {
            mediums.push(note.clone());
        }
    }

    let medium_included = mediums.len() >= config.auditor_include_medium_min;
    let mut track_a = highs;
    if medium_included {
        track_a.extend(mediums.clone());
    }
    let medium_skipped = if medium_included {
        0
    } else {
        mediums.len()
    };

    AuditorFindingSelection {
        track_a,
        medium_skipped,
        medium_included,
    }
}

fn format_note_row(note: &NoteLine) -> String {
    let file = note.file.as_deref().unwrap_or("?");
    let line = note
        .line
        .map(|l| {
            if let Some(end) = note.line_end {
                format!(":{l}-{end}")
            } else {
                format!(":{l}")
            }
        })
        .unwrap_or_default();
    let title = note.title.as_deref().unwrap_or("");
    let claim = note.claim.as_deref().unwrap_or("");
    let sev = note.severity.as_deref().unwrap_or("?");
    format!(
        "| {} | {} | `{file}{line}` | {title} | {claim} |",
        note.id, sev, 
    )
}

/// Build the scratchpad-backed sections prepended to an Auditor spawn prompt.
#[must_use]
pub fn build_auditor_assignment_sections(
    workspace: &Path,
    run_id: &str,
    report_draft: &str,
    config: &ScratchpadConfig,
) -> Option<String> {
    if !config.enabled || !config.auditor_from_scratchpad {
        return None;
    }
    let store = try_open_store(workspace, Some(run_id), None, None)?;
    let inventory = store.read_inventory().ok()?;
    let notes = store.read_notes().ok()?;
    let selection = select_auditor_findings(&notes, config);
    let stats = compute_coverage_stats(&inventory, &notes, config);

    let mut out = String::new();
    out.push_str("## Scratchpad source of truth (Phase C2)\n\n");
    out.push_str(&format!(
        "run_id: `{run_id}` · path: `{}` · verified_findings: {}\n\n",
        display_run_path(run_id),
        stats.verified_findings,
    ));

    out.push_str("### Track A — mechanical audit (use `note_id`, not prose numbering)\n\n");
    if selection.track_a.is_empty() {
        out.push_str("_No HIGH/BLOCKER/MEDIUM findings in scratchpad for track A._\n\n");
    } else {
        out.push_str("| note_id | severity | location | title | claim |\n");
        out.push_str("|---------|----------|----------|-------|-------|\n");
        for note in &selection.track_a {
            out.push_str(&format_note_row(note));
            out.push('\n');
        }
        out.push('\n');
        out.push_str(
            "For **each row**, run path check → line check → symbol containment on cited lines. \
             FAIL must cite `note_id`.\n\n",
        );
    }

    if selection.medium_skipped > 0 {
        out.push_str(&format!(
            "**Note:** {} MEDIUM finding(s) in scratchpad are **not** in track A (below \
             `auditor_include_medium_min={}`). Parent must self-verify those with read_file/grep \
             before publishing.\n\n",
            selection.medium_skipped, config.auditor_include_medium_min,
        ));
    }

    out.push_str("### Track B — prose draft cross-check\n\n");
    out.push_str(
        "The parent's prose report draft follows. Every **HIGH/MEDIUM** claim in prose must have \
         a matching `note_id` in track A. If prose cites a finding with no scratchpad `note_id`, \
         record `UNVERIFIED_CLAIM` (FAIL).\n\n",
    );
    out.push_str("```markdown\n");
    out.push_str(report_draft.trim());
    out.push_str("\n```\n");

    Some(out)
}

/// Resolve run_id from explicit input or runtime slot.
#[must_use]
pub fn resolve_auditor_run_id(
    explicit: Option<&str>,
    runtime_slot: Option<&str>,
) -> Option<String> {
    explicit
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            runtime_slot
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratchpad::schema::parse_note_line;
    use serde_json::json;

    #[test]
    fn medium_included_at_threshold() {
        let mut notes = Vec::new();
        for i in 1..=3 {
            notes.push(parse_note_line(
                &json!({
                    "id": format!("note-{i}"),
                    "kind": "finding",
                    "status": "verified",
                    "severity": "MEDIUM",
                    "area_id": "a",
                    "file": "f.rs",
                    "line": i
                }),
                i as usize,
            ));
        }
        let cfg = ScratchpadConfig::default();
        let sel = select_auditor_findings(&notes, &cfg);
        assert_eq!(sel.track_a.len(), 3);
        assert!(sel.medium_included);
    }

    #[test]
    fn medium_skipped_below_threshold() {
        let notes = vec![parse_note_line(
            &json!({
                "id": "note-1",
                "kind": "finding",
                "status": "verified",
                "severity": "MEDIUM",
                "area_id": "a"
            }),
            1,
        )];
        let cfg = ScratchpadConfig::default();
        let sel = select_auditor_findings(&notes, &cfg);
        assert!(sel.track_a.is_empty());
        assert_eq!(sel.medium_skipped, 1);
    }
}
