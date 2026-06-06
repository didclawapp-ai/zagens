//! Plan ↔ checklist consistency guard (P1-5 / P1d).

use crate::tools::plan::{PlanSnapshot, StepStatus};
use crate::tools::todo::{TodoListSnapshot, TodoStatus};

use super::verify::strip_verify_prefix;

/// Completed checklist items that name a plan phase still `pending` / `in_progress`.
#[must_use]
pub fn find_plan_checklist_drift(plan: &PlanSnapshot, checklist: &TodoListSnapshot) -> Vec<String> {
    if checklist.items.is_empty() {
        return Vec::new();
    }
    if !checklist
        .items
        .iter()
        .all(|i| i.status == TodoStatus::Completed)
    {
        return Vec::new();
    }

    let mut drift = Vec::new();
    for (idx, phase) in plan.items.iter().enumerate() {
        if phase.status == StepStatus::Completed {
            continue;
        }
        let phase_num = idx + 1;
        let markers = [
            format!("Phase {phase_num}"),
            format!("phase {phase_num}"),
            format!("P{phase_num}"),
            format!("P{phase_num}:"),
            format!("P{phase_num} "),
            phase.step.clone(),
        ];
        for item in &checklist.items {
            let content = strip_verify_prefix(&item.content).to_lowercase();
            if markers
                .iter()
                .any(|m| !m.is_empty() && content.contains(&m.to_lowercase()))
            {
                drift.push(format!(
                    "plan「{}」仍为 {:?}，但 checklist 已勾选含该阶段的项",
                    phase.step, phase.status
                ));
                break;
            }
        }
    }
    drift
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::plan::PlanItemArg;
    use crate::tools::todo::TodoItem;

    #[test]
    fn drift_when_checklist_names_pending_phase() {
        let plan = PlanSnapshot {
            explanation: None,
            items: vec![
                PlanItemArg {
                    step: "IPC migration".into(),
                    status: StepStatus::Pending,
                },
                PlanItemArg {
                    step: "Frontend wiring".into(),
                    status: StepStatus::Completed,
                },
            ],
        };
        let checklist = TodoListSnapshot {
            items: vec![TodoItem {
                id: 1,
                content: "Phase 1 IPC migration done".into(),
                status: TodoStatus::Completed,
            }],
            completion_pct: 100,
            in_progress_id: None,
        };
        let drift = find_plan_checklist_drift(&plan, &checklist);
        assert_eq!(drift.len(), 1);
        assert!(drift[0].contains("IPC migration"));
    }

    #[test]
    fn no_drift_when_plan_matches() {
        let plan = PlanSnapshot {
            explanation: None,
            items: vec![PlanItemArg {
                step: "Ship".into(),
                status: StepStatus::Completed,
            }],
        };
        let checklist = TodoListSnapshot {
            items: vec![TodoItem {
                id: 1,
                content: "Phase 1 ship".into(),
                status: TodoStatus::Completed,
            }],
            completion_pct: 100,
            in_progress_id: None,
        };
        assert!(find_plan_checklist_drift(&plan, &checklist).is_empty());
    }
}
