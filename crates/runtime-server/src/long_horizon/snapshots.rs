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
