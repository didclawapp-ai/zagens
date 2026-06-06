//! Parse persisted plan/checklist snapshots for HTTP task-graph (engine evicted).

use crate::tools::plan::PlanSnapshot;
use crate::tools::todo::TodoListSnapshot;

#[must_use]
pub fn empty_plan_snapshot() -> PlanSnapshot {
    PlanSnapshot {
        explanation: None,
        items: vec![],
    }
}

#[must_use]
pub fn empty_checklist_snapshot() -> TodoListSnapshot {
    TodoListSnapshot {
        items: vec![],
        completion_pct: 0,
        in_progress_id: None,
    }
}

#[must_use]
pub fn plan_from_json(value: Option<&serde_json::Value>) -> PlanSnapshot {
    let Some(v) = value else {
        return empty_plan_snapshot();
    };
    if let Ok(plan) = serde_json::from_value::<PlanSnapshot>(v.clone()) {
        return plan;
    }
    if let Some(inner) = v.get("plan") {
        if let Ok(plan) = serde_json::from_value::<PlanSnapshot>(inner.clone()) {
            return plan;
        }
    }
    empty_plan_snapshot()
}

#[must_use]
pub fn checklist_from_json(value: Option<&serde_json::Value>) -> TodoListSnapshot {
    let Some(v) = value else {
        return empty_checklist_snapshot();
    };
    if let Ok(snap) = serde_json::from_value::<TodoListSnapshot>(v.clone()) {
        return snap;
    }
    if let Some(inner) = v.get("checklist") {
        if let Ok(snap) = serde_json::from_value::<TodoListSnapshot>(inner.clone()) {
            return snap;
        }
    }
    empty_checklist_snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::todo::{TodoItem, TodoStatus};

    /// Contract behind the CCR progress-desync fix: the engine pushes its live
    /// checklist as `serde_json::to_string(&TodoListSnapshot)` over the harness
    /// status channel, and the host persists that string verbatim. The UI's
    /// task-graph read path then parses it via [`checklist_from_json`]. This
    /// round-trip MUST preserve item ids, statuses, completion %, and the
    /// in-progress id — otherwise the authoritative sync would silently corrupt
    /// the persisted checklist instead of repairing it.
    #[test]
    fn engine_serialized_snapshot_round_trips_through_checklist_from_json() {
        let snap = TodoListSnapshot {
            items: vec![
                TodoItem {
                    id: 1,
                    content: "done item".into(),
                    status: TodoStatus::Completed,
                },
                TodoItem {
                    id: 2,
                    content: "active item".into(),
                    status: TodoStatus::InProgress,
                },
                TodoItem {
                    id: 3,
                    content: "todo item".into(),
                    status: TodoStatus::Pending,
                },
            ],
            completion_pct: 33,
            in_progress_id: Some(2),
        };

        let json = serde_json::to_string(&snap).expect("serialize snapshot");
        let value: serde_json::Value = serde_json::from_str(&json).expect("reparse");
        let parsed = checklist_from_json(Some(&value));

        assert_eq!(parsed.items.len(), 3);
        assert_eq!(parsed.completion_pct, 33);
        assert_eq!(parsed.in_progress_id, Some(2));
        assert_eq!(parsed.items[0].status, TodoStatus::Completed);
        assert_eq!(parsed.items[1].status, TodoStatus::InProgress);
        assert_eq!(parsed.items[2].id, 3);
        assert_eq!(parsed.items[0].content, "done item");
    }
}
