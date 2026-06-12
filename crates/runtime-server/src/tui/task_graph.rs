//! LHT task-graph snapshot for the right-rail lower pane (no completion gate UI).

use serde::Deserialize;
use serde_json::Value;

use super::harness::{ChecklistItem, ChecklistSnapshot, ChecklistStatus, parse_status};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskGraphSnapshot {
    pub objective: String,
    pub phases: Vec<PhaseItem>,
    pub checklist: Vec<ChecklistItem>,
    pub completion_pct: u8,
    pub open_items: u32,
    pub in_progress_id: Option<u32>,
    pub lht_blocked: bool,
    pub nudge_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseItem {
    pub step: String,
    pub status: PhaseStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhaseStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Deserialize)]
struct RawTaskGraph {
    #[serde(default)]
    objective: String,
    #[serde(default)]
    phases: Vec<RawPhase>,
    #[serde(default)]
    checklist: Vec<RawChecklistRow>,
    #[serde(default)]
    completion_pct: Option<f64>,
    #[serde(default)]
    open_items: Option<u32>,
    #[serde(default)]
    in_progress_id: Option<u32>,
    #[serde(default)]
    lht_blocked: Option<bool>,
    #[serde(default)]
    nudge_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RawPhase {
    #[serde(default)]
    step: String,
    #[serde(default)]
    status: String,
}

#[derive(Debug, Deserialize)]
struct RawChecklistRow {
    #[serde(default)]
    id: u32,
    #[serde(default)]
    content: String,
    #[serde(default)]
    status: String,
}

impl TaskGraphSnapshot {
    pub fn has_activity(&self) -> bool {
        !self.checklist.is_empty() || !self.phases.is_empty()
    }

    pub fn from_checklist(checklist: ChecklistSnapshot) -> Self {
        let open_items = checklist
            .items
            .iter()
            .filter(|i| i.status != ChecklistStatus::Completed)
            .count() as u32;
        Self {
            checklist: checklist.items,
            completion_pct: checklist.completion_pct,
            open_items,
            in_progress_id: checklist.in_progress_id,
            ..Self::default()
        }
    }

    pub fn merge_checklist(&mut self, checklist: ChecklistSnapshot) {
        self.checklist = checklist.items;
        self.completion_pct = checklist.completion_pct;
        self.in_progress_id = checklist.in_progress_id;
        self.open_items = self
            .checklist
            .iter()
            .filter(|i| i.status != ChecklistStatus::Completed)
            .count() as u32;
    }

    pub fn checklist_summary(&self) -> Option<ChecklistSnapshot> {
        if self.checklist.is_empty() {
            return None;
        }
        Some(ChecklistSnapshot {
            items: self.checklist.clone(),
            completion_pct: self.completion_pct,
            in_progress_id: self.in_progress_id,
        })
    }
}

pub fn parse_task_graph_value(value: &Value) -> Option<TaskGraphSnapshot> {
    snapshot_from_raw(serde_json::from_value(value.clone()).ok()?)
}

pub fn parse_task_graph_panel_payload(payload: &Value) -> Option<TaskGraphSnapshot> {
    let graph = payload.get("task_graph")?;
    parse_task_graph_value(graph)
}

fn snapshot_from_raw(raw: RawTaskGraph) -> Option<TaskGraphSnapshot> {
    let phases: Vec<PhaseItem> = raw
        .phases
        .into_iter()
        .filter(|p| !p.step.trim().is_empty())
        .map(|p| PhaseItem {
            step: p.step,
            status: parse_phase_status(&p.status),
        })
        .collect();
    let checklist: Vec<ChecklistItem> = raw
        .checklist
        .into_iter()
        .map(|row| ChecklistItem {
            id: row.id,
            content: row.content,
            status: parse_status(&row.status),
        })
        .collect();
    if phases.is_empty() && checklist.is_empty() {
        return None;
    }
    let completed = checklist
        .iter()
        .filter(|i| i.status == ChecklistStatus::Completed)
        .count();
    let total = checklist.len();
    let in_progress_id = raw.in_progress_id.or_else(|| {
        checklist
            .iter()
            .find(|i| i.status == ChecklistStatus::InProgress)
            .map(|i| i.id)
    });
    Some(TaskGraphSnapshot {
        objective: raw.objective,
        phases,
        checklist,
        completion_pct: raw
            .completion_pct
            .map(|p| p.round() as u8)
            .unwrap_or_else(|| {
                if total == 0 {
                    0
                } else {
                    ((completed as f64 / total as f64) * 100.0).round() as u8
                }
            }),
        open_items: raw
            .open_items
            .unwrap_or_else(|| (total.saturating_sub(completed)) as u32),
        in_progress_id,
        lht_blocked: raw.lht_blocked.unwrap_or(false),
        nudge_count: raw.nudge_count.unwrap_or(0),
    })
}

fn parse_phase_status(raw: &str) -> PhaseStatus {
    match raw.trim().to_ascii_lowercase().as_str() {
        "completed" | "done" => PhaseStatus::Completed,
        "in_progress" | "in-progress" | "active" => PhaseStatus::InProgress,
        _ => PhaseStatus::Pending,
    }
}

pub fn title_bar_harness_line_from_graph(graph: Option<&TaskGraphSnapshot>) -> String {
    let Some(graph) = graph else {
        return "LHT —".to_string();
    };
    if !graph.checklist.is_empty() {
        let total = graph.checklist.len();
        let completed = graph
            .checklist
            .iter()
            .filter(|i| i.status == ChecklistStatus::Completed)
            .count();
        let open = graph.open_items.max(total.saturating_sub(completed) as u32);
        return format!("LHT {completed}/{total} · open:{open}");
    }
    if !graph.phases.is_empty() {
        let total = graph.phases.len();
        let completed = graph
            .phases
            .iter()
            .filter(|p| p.status == PhaseStatus::Completed)
            .count();
        return format!("LHT {completed}/{total} · open:{}", total - completed);
    }
    "LHT —".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_task_graph_panel_payload_parses_phases_and_checklist() {
        let payload = serde_json::json!({
            "task_graph": {
                "objective": "Ship feature",
                "phases": [
                    {"step": "Design", "status": "completed"},
                    {"step": "Implement", "status": "in_progress"}
                ],
                "checklist": [
                    {"id": 1, "content": "tests", "status": "pending"}
                ],
                "completion_pct": 0,
                "open_items": 1,
                "lht_blocked": false,
                "nudge_count": 2
            }
        });
        let snap = parse_task_graph_panel_payload(&payload).expect("parse");
        assert_eq!(snap.objective, "Ship feature");
        assert_eq!(snap.phases.len(), 2);
        assert_eq!(snap.checklist.len(), 1);
        assert_eq!(snap.nudge_count, 2);
        assert!(snap.has_activity());
    }
}
