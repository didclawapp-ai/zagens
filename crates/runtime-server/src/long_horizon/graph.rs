//! Derived code-task graph from plan + checklist snapshots (LHT Phase 1).

use crate::tools::plan::{PlanSnapshot, StepStatus};
use crate::tools::todo::{TodoListSnapshot, TodoStatus};

/// Read-only view of plan + checklist progress for LHT gates.
#[derive(Debug, Clone)]
pub struct CodeTaskGraph {
    pub objective: String,
    pub objective_source: &'static str,
    pub phases: Vec<GraphPhase>,
    pub checklist: Vec<GraphChecklistItem>,
    pub completion_pct: u8,
    pub open_items: u32,
    pub in_progress_id: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct GraphPhase {
    pub step: String,
    pub status: StepStatus,
}

#[derive(Debug, Clone)]
pub struct GraphChecklistItem {
    pub id: u32,
    pub content: String,
    pub status: TodoStatus,
}

impl CodeTaskGraph {
    #[must_use]
    pub fn from_snapshots(plan: &PlanSnapshot, checklist: &TodoListSnapshot) -> Self {
        let phases: Vec<GraphPhase> = plan
            .items
            .iter()
            .map(|item| GraphPhase {
                step: item.step.clone(),
                status: item.status.clone(),
            })
            .collect();

        let checklist_items: Vec<GraphChecklistItem> = checklist
            .items
            .iter()
            .map(|item| GraphChecklistItem {
                id: item.id,
                content: item.content.clone(),
                status: item.status,
            })
            .collect();

        // Option 1 — checklist is the completion authority. When the model
        // executes via the checklist, the plan is only a high-level outline and
        // must NOT be counted as separate work: a plan drafted then abandoned
        // (all-pending "zombie" steps) would otherwise inflate the denominator,
        // wedge progress, and keep `incomplete()`/`open_items` non-zero forever
        // (DEMO5 root cause). Only fall back to the plan when the checklist is
        // empty (plan-only tasks keep their original behaviour).
        let checklist_authoritative = !checklist_items.is_empty();

        let (total, completed, open_items) = if checklist_authoritative {
            let total = checklist_items.len();
            let completed = checklist_items
                .iter()
                .filter(|c| c.status == TodoStatus::Completed)
                .count();
            (total, completed, (total - completed) as u32)
        } else {
            let total = phases.len();
            let completed = phases
                .iter()
                .filter(|p| p.status == StepStatus::Completed)
                .count();
            (total, completed, (total - completed) as u32)
        };
        let completion_pct = if total == 0 {
            100
        } else {
            ((completed * 100) / total).min(100) as u8
        };

        // When the checklist is authoritative, the in-progress pointer must come
        // from the checklist only — never fall back to a stale plan InProgress
        // step, or the nudge gate would re-target an abandoned plan item.
        let in_progress_id = if checklist_authoritative {
            checklist.in_progress_id
        } else {
            phases
                .iter()
                .position(|p| p.status == StepStatus::InProgress)
                .map(plan_in_progress_key)
        };

        Self {
            objective: String::new(),
            objective_source: "pending",
            phases,
            checklist: checklist_items,
            completion_pct,
            open_items,
            in_progress_id,
        }
    }

    /// Graph has no plan steps and no checklist items.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.phases.is_empty() && self.checklist.is_empty()
    }

    /// Trivial single-step task (at most one tracked item total).
    #[must_use]
    pub fn is_trivial(&self) -> bool {
        self.phases.len() + self.checklist.len() <= 1
    }

    #[must_use]
    pub fn incomplete(&self) -> bool {
        if self.is_empty() {
            return false;
        }
        // Checklist is the completion authority (Option 1, see `from_snapshots`):
        // when it is non-empty, a fully-checked checklist means done regardless
        // of any abandoned all-pending plan steps. Fall back to the plan only
        // when there is no checklist at all.
        if !self.checklist.is_empty() {
            self.checklist
                .iter()
                .any(|c| c.status != TodoStatus::Completed)
        } else {
            self.phases
                .iter()
                .any(|p| p.status != StepStatus::Completed)
        }
    }
}

/// Plan-only nudge tracker key (§4.3).
#[must_use]
pub fn plan_in_progress_key(plan_index: usize) -> u32 {
    0xFFFF_0000u32.wrapping_add(plan_index as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::plan::{PlanItemArg, PlanSnapshot};
    use crate::tools::todo::{TodoItem, TodoListSnapshot};

    #[test]
    fn empty_graph_not_incomplete() {
        let g = CodeTaskGraph::from_snapshots(&empty_plan(), &empty_checklist());
        assert!(!g.incomplete());
        assert!(g.is_empty());
    }

    #[test]
    fn plan_only_incomplete() {
        let plan = PlanSnapshot {
            explanation: Some("Refactor auth".into()),
            items: vec![PlanItemArg {
                step: "Introduce trait".into(),
                status: StepStatus::InProgress,
            }],
        };
        let g = CodeTaskGraph::from_snapshots(&plan, &empty_checklist());
        assert!(g.incomplete());
        assert_eq!(g.open_items, 1);
    }

    #[test]
    fn all_completed_not_incomplete() {
        let plan = PlanSnapshot {
            explanation: None,
            items: vec![PlanItemArg {
                step: "Done".into(),
                status: StepStatus::Completed,
            }],
        };
        let checklist = TodoListSnapshot {
            items: vec![TodoItem {
                id: 1,
                content: "Test".into(),
                status: TodoStatus::Completed,
            }],
            completion_pct: 100,
            in_progress_id: None,
        };
        let g = CodeTaskGraph::from_snapshots(&plan, &checklist);
        assert!(!g.incomplete());
        assert_eq!(g.completion_pct, 100);
    }

    #[test]
    fn checklist_authoritative_ignores_abandoned_plan() {
        // DEMO5 root case: a plan drafted (all pending) then abandoned while
        // the work is executed and finished via the checklist. The plan's
        // zombie pending steps must NOT count as open work.
        let plan = PlanSnapshot {
            explanation: Some("outline".into()),
            items: (0..12)
                .map(|i| PlanItemArg {
                    step: format!("phase {i}"),
                    status: StepStatus::Pending,
                })
                .collect(),
        };
        let checklist = TodoListSnapshot {
            items: (1..=19)
                .map(|id| TodoItem {
                    id,
                    content: format!("item {id}"),
                    status: TodoStatus::Completed,
                })
                .collect(),
            completion_pct: 100,
            in_progress_id: None,
        };
        let g = CodeTaskGraph::from_snapshots(&plan, &checklist);
        assert_eq!(g.completion_pct, 100);
        assert_eq!(g.open_items, 0);
        assert!(!g.incomplete());
        // Must not re-target an abandoned plan InProgress step.
        assert_eq!(g.in_progress_id, None);
    }

    #[test]
    fn checklist_authoritative_partial_progress() {
        // Plan all pending, checklist partway done → progress reflects the
        // checklist only (plan steps do not inflate the denominator).
        let plan = PlanSnapshot {
            explanation: None,
            items: vec![
                PlanItemArg {
                    step: "a".into(),
                    status: StepStatus::Pending,
                },
                PlanItemArg {
                    step: "b".into(),
                    status: StepStatus::Pending,
                },
            ],
        };
        let checklist = TodoListSnapshot {
            items: vec![
                TodoItem {
                    id: 1,
                    content: "done".into(),
                    status: TodoStatus::Completed,
                },
                TodoItem {
                    id: 2,
                    content: "wip".into(),
                    status: TodoStatus::InProgress,
                },
            ],
            completion_pct: 50,
            in_progress_id: Some(2),
        };
        let g = CodeTaskGraph::from_snapshots(&plan, &checklist);
        assert_eq!(g.completion_pct, 50);
        assert_eq!(g.open_items, 1);
        assert!(g.incomplete());
        assert_eq!(g.in_progress_id, Some(2));
    }

    fn empty_plan() -> PlanSnapshot {
        PlanSnapshot {
            explanation: None,
            items: vec![],
        }
    }

    fn empty_checklist() -> TodoListSnapshot {
        TodoListSnapshot {
            items: vec![],
            completion_pct: 0,
            in_progress_id: None,
        }
    }
}
