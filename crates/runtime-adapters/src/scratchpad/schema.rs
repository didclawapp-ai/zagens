//! Audit scratchpad JSON schemas (`inventory.json`, `notes.jsonl` lines).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Inventory area status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaStatus {
    Pending,
    InProgress,
    Done,
    Deferred,
}

impl AreaStatus {
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "in_progress" | "inprogress" => Some(Self::InProgress),
            "done" => Some(Self::Done),
            "deferred" => Some(Self::Deferred),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Done => "done",
            Self::Deferred => "deferred",
        }
    }
}

/// One row in `inventory.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryArea {
    pub id: String,
    pub path: String,
    pub status: AreaStatus,
    #[serde(default)]
    pub notes: String,
    /// Directory has many `fn`/`impl_fn` symbols (best-effort; omitted when index missing).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub high_complexity: bool,
}

/// `inventory.json` root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub areas: Vec<InventoryArea>,
}

/// Parsed `notes.jsonl` line (Phase A fields optional).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteLine {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub ts: String,
    #[serde(default)]
    pub area_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    #[serde(default)]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_end: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
}

/// Normalize a raw JSON object from disk or tool input into a [`NoteLine`].
pub fn parse_note_line(raw: &Value, line_no: usize) -> NoteLine {
    let mut note: NoteLine = serde_json::from_value(raw.clone()).unwrap_or_else(|_| NoteLine {
        id: String::new(),
        ts: String::new(),
        area_id: String::new(),
        area: None,
        kind: "meta".to_string(),
        severity: None,
        title: None,
        file: None,
        line: None,
        line_end: None,
        claim: None,
        evidence: None,
        status: String::new(),
        source: None,
        supersedes: None,
    });

    if note.id.is_empty() {
        note.id = format!("legacy-{line_no}");
    }
    if note.ts.is_empty() {
        note.ts = String::new();
    }
    if note.kind.is_empty() {
        note.kind = raw
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("meta")
            .to_string();
    }
    if note.status.is_empty() {
        note.status = match note.kind.as_str() {
            "finding" => "verified".to_string(),
            "todo" => "open".to_string(),
            _ => "open".to_string(),
        };
    }
    if note.area_id.is_empty() {
        note.area_id = raw
            .get("area_id")
            .and_then(|v| v.as_str())
            .unwrap_or("_global")
            .to_string();
    }
    note
}

/// Severity strings treated as HIGH tier for validation and L1 summary.
#[must_use]
pub fn is_high_severity(severity: Option<&str>) -> bool {
    matches!(
        severity.map(str::to_uppercase).as_deref(),
        Some("HIGH") | Some("BLOCKER")
    )
}

/// Whether this note counts as a verified finding for status tallies.
#[must_use]
pub fn is_verified_finding(note: &NoteLine, superseded: &HashSet<String>) -> bool {
    note.kind == "finding"
        && note.status.eq_ignore_ascii_case("verified")
        && !superseded.contains(&note.id)
}

/// Whether this note counts as an open finding.
#[must_use]
pub fn is_open_finding(note: &NoteLine, superseded: &HashSet<String>) -> bool {
    note.kind == "finding"
        && note.status.eq_ignore_ascii_case("open")
        && !superseded.contains(&note.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_phase_a_line_defaults() {
        let raw = json!({
            "kind": "cleared",
            "area_id": "area-core",
            "claim": "ok"
        });
        let note = parse_note_line(&raw, 3);
        assert_eq!(note.id, "legacy-3");
        assert_eq!(note.area_id, "area-core");
        assert_eq!(note.kind, "cleared");
        assert_eq!(note.status, "open");
        assert_eq!(note.kind, "cleared");
    }

    #[test]
    fn finding_defaults_verified_status() {
        let raw = json!({
            "id": "note-1",
            "kind": "finding",
            "area_id": "a",
            "severity": "LOW"
        });
        let note = parse_note_line(&raw, 1);
        assert_eq!(note.status, "verified");
    }
}
