//! Plan ↔ checklist sync warnings on tool results (TS-10).

use crate::long_horizon::find_plan_checklist_drift;
use crate::tools::plan::PlanSnapshot;
use crate::tools::spec::ToolResult;
use crate::tools::todo::TodoListSnapshot;
use serde_json::json;

/// Short block appended to `checklist_update` / `checklist_write` / `update_plan` results.
#[must_use]
pub fn format_plan_checklist_sync_warning(drift: &[String]) -> String {
    let list = drift
        .iter()
        .map(|s| format!("  - {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "\n[SYNC_WARNING] Checklist is fully completed but plan phases are still open for the same work:\n{list}\n\
         Use `update_plan` to mark finished phases `completed`, or revert checklist items. \
         Progress SSOT: when checklist is non-empty, harness completion % follows checklist only (plan outline must stay in sync)."
    )
}

#[must_use]
pub fn plan_checklist_sync_warning(
    plan: &PlanSnapshot,
    checklist: &TodoListSnapshot,
) -> Option<String> {
    let drift = find_plan_checklist_drift(plan, checklist);
    if drift.is_empty() {
        None
    } else {
        Some(format_plan_checklist_sync_warning(&drift))
    }
}

pub fn append_plan_checklist_sync_warning(
    mut result: ToolResult,
    plan: &PlanSnapshot,
    checklist: &TodoListSnapshot,
) -> ToolResult {
    if let Some(warn) = plan_checklist_sync_warning(plan, checklist) {
        result.content.push_str(&warn);
        if let Some(meta) = result.metadata.as_mut()
            && let Some(obj) = meta.as_object_mut()
        {
            obj.insert(
                "plan_checklist_sync_warning".to_string(),
                json!({
                    "drift_items": find_plan_checklist_drift(plan, checklist),
                }),
            );
        } else {
            result.metadata = Some(json!({
                "plan_checklist_sync_warning": {
                    "drift_items": find_plan_checklist_drift(plan, checklist),
                }
            }));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::plan::{PlanItemArg, StepStatus, UpdatePlanTool, new_shared_plan_state};
    use crate::tools::spec::ToolContext;
    use crate::tools::spec::ToolSpec;
    use crate::tools::todo::{TodoItem, TodoStatus, TodoUpdateTool, new_shared_todo_list};
    use serde_json::json;

    #[test]
    fn sync_warning_includes_ssot_hint() {
        let plan = PlanSnapshot {
            explanation: None,
            items: vec![PlanItemArg {
                step: "Backend API".into(),
                status: StepStatus::Pending,
            }],
        };
        let checklist = TodoListSnapshot {
            items: vec![TodoItem {
                id: 1,
                content: "Phase 1 Backend API shipped".into(),
                status: TodoStatus::Completed,
            }],
            completion_pct: 100,
            in_progress_id: None,
        };
        let warn = plan_checklist_sync_warning(&plan, &checklist).expect("warn");
        assert!(warn.contains("[SYNC_WARNING]"));
        assert!(warn.contains("SSOT"));
        assert!(warn.contains("Backend API"));
    }

    #[tokio::test]
    async fn checklist_update_appends_sync_warning_when_plan_drifts() {
        let todo_list = new_shared_todo_list();
        let plan_state = new_shared_plan_state();
        {
            let mut plan = plan_state.lock().await;
            plan.update(crate::tools::plan::UpdatePlanArgs {
                explanation: None,
                plan: vec![PlanItemArg {
                    step: "Backend API".into(),
                    status: StepStatus::Pending,
                }],
            });
        }
        {
            let mut list = todo_list.lock().await;
            list.add("Phase 1 Backend API shipped".into(), TodoStatus::Completed);
        }

        let tool = TodoUpdateTool::checklist(todo_list, plan_state);
        let context = ToolContext::new(std::env::temp_dir());
        let result = tool
            .execute(json!({ "id": 1, "status": "completed" }), &context)
            .await
            .expect("update succeeds");

        assert!(result.content.contains("[SYNC_WARNING]"));
        assert!(
            result
                .metadata
                .as_ref()
                .and_then(|m| m.get("plan_checklist_sync_warning"))
                .is_some()
        );
    }

    #[tokio::test]
    async fn update_plan_appends_sync_warning_when_checklist_drifts() {
        let todo_list = new_shared_todo_list();
        let plan_state = new_shared_plan_state();
        {
            let mut list = todo_list.lock().await;
            list.add("Phase 1 Backend API shipped".into(), TodoStatus::Completed);
        }

        let tool = UpdatePlanTool::new(plan_state, todo_list);
        let context = ToolContext::new(std::env::temp_dir());
        let result = tool
            .execute(
                json!({
                    "plan": [
                        { "step": "Backend API", "status": "pending" }
                    ]
                }),
                &context,
            )
            .await
            .expect("plan update succeeds");

        assert!(result.content.contains("[SYNC_WARNING]"));
    }
}
