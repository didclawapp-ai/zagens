//! Harness checklist snapshot and TitleBar formatting (Phase 2).

use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChecklistSnapshot {
    pub items: Vec<ChecklistItem>,
    pub completion_pct: u8,
    pub in_progress_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistItem {
    pub id: u32,
    pub content: String,
    pub status: ChecklistStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecklistStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Deserialize)]
struct RawChecklist {
    #[serde(default)]
    items: Vec<RawChecklistItem>,
    #[serde(default)]
    completion_pct: Option<f64>,
    #[serde(default)]
    in_progress_id: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RawChecklistItem {
    #[serde(default)]
    id: u32,
    #[serde(default)]
    content: String,
    #[serde(default)]
    status: String,
}

pub fn parse_checklist_json(raw: &str) -> Option<ChecklistSnapshot> {
    let parsed: RawChecklist = serde_json::from_str(raw).ok()?;
    snapshot_from_raw(parsed)
}

/// Parse `panel.checklist` runtime payload (`{ "checklist": { items, … } }`).
pub fn parse_checklist_panel_payload(payload: &serde_json::Value) -> Option<ChecklistSnapshot> {
    let checklist = payload.get("checklist")?;
    let parsed: RawChecklist = serde_json::from_value(checklist.clone()).ok()?;
    snapshot_from_raw(parsed)
}

fn snapshot_from_raw(parsed: RawChecklist) -> Option<ChecklistSnapshot> {
    if parsed.items.is_empty() {
        return None;
    }
    let items: Vec<ChecklistItem> = parsed
        .items
        .into_iter()
        .map(|row| ChecklistItem {
            id: row.id,
            content: row.content,
            status: parse_status(&row.status),
        })
        .collect();
    let total = items.len();
    let completed = items
        .iter()
        .filter(|i| i.status == ChecklistStatus::Completed)
        .count();
    let in_progress = items
        .iter()
        .find(|i| i.status == ChecklistStatus::InProgress);
    Some(ChecklistSnapshot {
        completion_pct: parsed
            .completion_pct
            .map(|p| p.round() as u8)
            .unwrap_or_else(|| {
                if total == 0 {
                    0
                } else {
                    ((completed as f64 / total as f64) * 100.0).round() as u8
                }
            }),
        in_progress_id: parsed.in_progress_id.or_else(|| in_progress.map(|i| i.id)),
        items,
    })
}

pub(crate) fn parse_status(raw: &str) -> ChecklistStatus {
    match raw.trim().to_ascii_lowercase().as_str() {
        "completed" | "done" => ChecklistStatus::Completed,
        "in_progress" | "in-progress" | "active" => ChecklistStatus::InProgress,
        _ => ChecklistStatus::Pending,
    }
}

pub fn title_bar_harness_line(snapshot: Option<&ChecklistSnapshot>) -> String {
    let Some(snap) = snapshot else {
        return "LHT —".to_string();
    };
    if snap.items.is_empty() {
        return "LHT —".to_string();
    }
    let total = snap.items.len();
    let completed = snap
        .items
        .iter()
        .filter(|i| i.status == ChecklistStatus::Completed)
        .count();
    let open = total.saturating_sub(completed);
    format!("LHT {completed}/{total} · open:{open}")
}

pub fn blocked_suffix(end_reason: Option<&str>) -> Option<String> {
    let reason = end_reason?.trim();
    if reason.is_empty() {
        return None;
    }
    let lower = reason.to_ascii_lowercase();
    if lower.contains("blocked") || lower.contains("gate") {
        Some(format!("⏸ {reason}"))
    } else {
        None
    }
}

pub fn format_checklist_panel(snapshot: &ChecklistSnapshot, height: usize) -> Vec<String> {
    let mut lines = vec![format!(
        "Checklist {}% ({}/{})",
        snapshot.completion_pct,
        snapshot
            .items
            .iter()
            .filter(|i| i.status == ChecklistStatus::Completed)
            .count(),
        snapshot.items.len()
    )];
    for item in &snapshot.items {
        let mark = match item.status {
            ChecklistStatus::Completed => "[x]",
            ChecklistStatus::InProgress => "[>]",
            ChecklistStatus::Pending => "[ ]",
        };
        let content = truncate_line(&item.content, 48);
        lines.push(format!("{mark} {content}"));
    }
    if lines.len() > height.max(4) {
        let skip = lines.len() - height.max(4);
        lines.drain(1..skip + 1);
    }
    lines
}

fn truncate_line(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_panel_checklist_payload() {
        let payload = serde_json::json!({
            "checklist": {
                "items": [
                    {"id": 1, "content": "a", "status": "completed"},
                    {"id": 2, "content": "b", "status": "pending"}
                ],
                "completion_pct": 50
            }
        });
        let snap = parse_checklist_panel_payload(&payload).expect("parse");
        assert_eq!(snap.items.len(), 2);
        assert_eq!(snap.completion_pct, 50);
    }

    #[test]
    fn parse_checklist_and_title_bar() {
        let json = r#"{"items":[{"id":1,"content":"a","status":"completed"},{"id":2,"content":"b","status":"in_progress"},{"id":3,"content":"c","status":"pending"}]}"#;
        let snap = parse_checklist_json(json).expect("parse");
        assert_eq!(snap.items.len(), 3);
        assert_eq!(title_bar_harness_line(Some(&snap)), "LHT 1/3 · open:2");
    }

    #[test]
    fn blocked_suffix_detects_gate() {
        assert_eq!(
            blocked_suffix(Some("lht_blocked")).as_deref(),
            Some("⏸ lht_blocked")
        );
        assert!(blocked_suffix(Some("end_turn")).is_none());
    }
}
