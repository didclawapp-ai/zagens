//! LHT checkpoint scheduling after plan/checklist completions (Phase 2 §10.2).

use serde_json::Value;

use crate::tools::plan::StepStatus;
use crate::tools::todo::TodoStatus;

/// True when a successful tool call completed a plan step or checklist item.
#[must_use]
pub fn tool_marks_lht_checkpoint(tool_name: &str, tool_input: &Value, success: bool) -> bool {
    if !success {
        return false;
    }
    match tool_name {
        "checklist_update" | "todo_update" => tool_input
            .get("status")
            .and_then(|v| v.as_str())
            .and_then(TodoStatus::from_str)
            == Some(TodoStatus::Completed),
        "update_plan" => plan_input_marks_completion(tool_input),
        _ => false,
    }
}

fn plan_input_marks_completion(input: &Value) -> bool {
    let Some(steps) = input.get("steps").and_then(|v| v.as_array()) else {
        return input
            .get("status")
            .and_then(|v| v.as_str())
            .and_then(StepStatus::from_str)
            == Some(StepStatus::Completed);
    };
    steps.iter().any(|step| {
        step.get("status")
            .and_then(|v| v.as_str())
            .and_then(StepStatus::from_str)
            == Some(StepStatus::Completed)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn checklist_completed_marks_checkpoint() {
        assert!(tool_marks_lht_checkpoint(
            "checklist_update",
            &json!({ "id": 1, "status": "completed" }),
            true,
        ));
        assert!(!tool_marks_lht_checkpoint(
            "checklist_update",
            &json!({ "id": 1, "status": "in_progress" }),
            true,
        ));
    }
}
