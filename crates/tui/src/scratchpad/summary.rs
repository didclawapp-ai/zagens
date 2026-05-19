//! P2 layered summary and `supersedes` transitive closure.

use std::collections::{HashMap, HashSet};

use crate::scratchpad::schema::{Inventory, NoteLine, is_high_severity, is_verified_finding};

/// Ids excluded from P2 reports: every `supersedes` target, plus transitive closure
/// (if `note-4` supersedes `note-3` and `note-3` supersedes `note-2`, both `note-3` and `note-2` are out).
#[must_use]
pub fn compute_superseded_ids(notes: &[NoteLine]) -> HashSet<String> {
    let mut edges: Vec<(String, String)> = Vec::new();
    for note in notes {
        if let Some(target) = note.supersedes.as_ref().filter(|t| !t.is_empty()) {
            edges.push((note.id.clone(), target.clone()));
        }
    }

    let mut superseded: HashSet<String> = edges.iter().map(|(_, old)| old.clone()).collect();
    let mut changed = true;
    while changed {
        changed = false;
        for (new_id, old_id) in &edges {
            if superseded.contains(new_id) && superseded.insert(old_id.clone()) {
                changed = true;
            }
        }
    }
    superseded
}

/// Build layered P2 summary text (§6.2). Used by engine injection in B3.
#[must_use]
#[allow(dead_code)]
pub fn build_layered_summary(
    inventory: &Inventory,
    notes: &[NoteLine],
    max_chars: usize,
) -> String {
    let superseded = compute_superseded_ids(notes);
    let mut out = String::new();

    // L0
    let total = inventory.areas.len();
    let done = inventory
        .areas
        .iter()
        .filter(|a| matches!(a.status, crate::scratchpad::schema::AreaStatus::Done))
        .count();
    let deferred = inventory
        .areas
        .iter()
        .filter(|a| matches!(a.status, crate::scratchpad::schema::AreaStatus::Deferred))
        .count();
    let in_progress = inventory
        .areas
        .iter()
        .filter(|a| matches!(a.status, crate::scratchpad::schema::AreaStatus::InProgress))
        .count();
    let pending = inventory
        .areas
        .iter()
        .filter(|a| matches!(a.status, crate::scratchpad::schema::AreaStatus::Pending))
        .count();
    let resume = inventory
        .areas
        .iter()
        .find(|a| {
            matches!(
                a.status,
                crate::scratchpad::schema::AreaStatus::Pending
                    | crate::scratchpad::schema::AreaStatus::InProgress
            )
        })
        .map(|a| a.id.as_str())
        .unwrap_or("none");
    let verified = notes
        .iter()
        .filter(|n| is_verified_finding(n, &superseded))
        .count();
    let l0 = format!(
        "[L0] areas {done}/{total} done, {deferred} deferred, {in_progress} in_progress, {pending} pending; resume_area_id={resume}; verified_findings={verified}\n"
    );
    out.push_str(&l0);

    // L1 — HIGH/BLOCKER verified
    let mut omitted_high: Vec<String> = Vec::new();
    out.push_str("[L1] HIGH/BLOCKER verified findings:\n");
    for note in notes {
        if !is_verified_finding(note, &superseded) {
            continue;
        }
        if !is_high_severity(note.severity.as_deref()) {
            continue;
        }
        let line = format_finding_line(note);
        if out.len() + line.len() > max_chars {
            omitted_high.push(note.id.clone());
            continue;
        }
        out.push_str(&line);
        out.push('\n');
    }

    if !omitted_high.is_empty() {
        out.push_str(&format!("omitted_high_ids: {omitted_high:?}\n"));
    }

    // L2 — sample MEDIUM per area (max 2 each)
    if out.len() < max_chars {
        out.push_str("[L2] MEDIUM verified (sampled):\n");
        let mut per_area: HashMap<String, usize> = HashMap::new();
        for note in notes {
            if !is_verified_finding(note, &superseded) {
                continue;
            }
            let sev = note.severity.as_deref().unwrap_or("").to_uppercase();
            if sev != "MEDIUM" {
                continue;
            }
            let count = per_area.entry(note.area_id.clone()).or_insert(0);
            if *count >= 2 {
                continue;
            }
            let line = format_finding_line(note);
            if out.len() + line.len() > max_chars {
                break;
            }
            out.push_str(&line);
            out.push('\n');
            *count += 1;
        }
    }

    if out.len() > max_chars {
        out.truncate(max_chars);
    }
    out
}

fn format_finding_line(note: &NoteLine) -> String {
    let file = note.file.as_deref().unwrap_or("?");
    let line = note
        .line
        .map(|l| format!(":{l}"))
        .unwrap_or_default();
    let claim = note.claim.as_deref().unwrap_or("");
    format!(
        "- [{}] {}`{}{}` — {}",
        note.id, note.area_id, file, line, claim
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratchpad::schema::{AreaStatus, Inventory, InventoryArea, parse_note_line};
    use serde_json::json;

    #[test]
    fn transitive_supersedes_when_head_is_also_superseded() {
        let notes = vec![
            parse_note_line(
                &json!({"id":"note-6","supersedes":"note-5","kind":"finding","area_id":"a"}),
                6,
            ),
            parse_note_line(
                &json!({"id":"note-5","supersedes":"note-4","kind":"finding","area_id":"a"}),
                5,
            ),
            parse_note_line(
                &json!({"id":"note-4","supersedes":"note-3","kind":"finding","area_id":"a"}),
                4,
            ),
            parse_note_line(&json!({"id":"note-3","kind":"finding","area_id":"a"}), 3),
        ];
        let s = compute_superseded_ids(&notes);
        assert!(s.contains("note-5"));
        assert!(s.contains("note-4"));
        assert!(s.contains("note-3"));
        assert!(!s.contains("note-6"));
    }

    #[test]
    fn transitive_supersedes() {
        let notes = vec![
            parse_note_line(
                &json!({"id":"note-3","supersedes":"note-2","kind":"finding","area_id":"a"}),
                3,
            ),
            parse_note_line(
                &json!({"id":"note-2","supersedes":"note-1","kind":"finding","area_id":"a"}),
                2,
            ),
            parse_note_line(&json!({"id":"note-1","kind":"finding","area_id":"a"}), 1),
        ];
        let s = compute_superseded_ids(&notes);
        assert!(s.contains("note-1"));
        assert!(s.contains("note-2"));
        assert!(!s.contains("note-3"));
    }

    #[test]
    fn layered_summary_includes_l0() {
        let inv = Inventory {
            run_id: "r".into(),
            created_at: String::new(),
            completed_at: None,
            scope: None,
            areas: vec![InventoryArea {
                id: "area-a".into(),
                path: "src/".into(),
                status: AreaStatus::Done,
                notes: String::new(),
            }],
        };
        let notes = vec![parse_note_line(
            &json!({
                "id":"note-1",
                "kind":"finding",
                "status":"verified",
                "severity":"HIGH",
                "area_id":"area-a",
                "file":"a.rs",
                "line":1,
                "claim":"bug"
            }),
            1,
        )];
        let s = build_layered_summary(&inv, &notes, 8000);
        assert!(s.contains("[L0]"));
        assert!(s.contains("note-1"));
    }
}
