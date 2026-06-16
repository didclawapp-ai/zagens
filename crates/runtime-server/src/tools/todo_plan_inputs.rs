//! schemars input types for the todo/checklist and plan tool families (kernel-v2 M2).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::tools::tool_schema::derived_input_schema;

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ChecklistStatusInput {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct ChecklistAddInput {
    #[schemars(description = "The task description")]
    pub content: String,
    #[schemars(description = "Task status (default: pending)")]
    pub status: Option<ChecklistStatusInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct ChecklistUpdateInput {
    #[schemars(description = "Todo item id")]
    pub id: u64,
    #[schemars(description = "New status")]
    pub status: ChecklistStatusInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct ChecklistListInput {}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct ChecklistWriteItemInput {
    #[schemars(description = "The task description")]
    pub content: String,
    #[schemars(description = "Task status")]
    pub status: ChecklistStatusInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct ChecklistWriteInput {
    #[schemars(description = "The complete list of todo items. This replaces the existing list.")]
    pub todos: Vec<ChecklistWriteItemInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct PlanItemInput {
    #[schemars(description = "Description of the step")]
    pub step: String,
    #[schemars(description = "Step status")]
    pub status: ChecklistStatusInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct UpdatePlanInput {
    #[schemars(description = "Optional high-level explanation of the plan or approach")]
    pub explanation: Option<String>,
    #[schemars(description = "List of plan steps")]
    pub plan: Vec<PlanItemInput>,
}

#[must_use]
pub fn checklist_add_input_schema() -> Value {
    derived_input_schema::<ChecklistAddInput>()
}

#[must_use]
pub fn checklist_update_input_schema() -> Value {
    derived_input_schema::<ChecklistUpdateInput>()
}

#[must_use]
pub fn checklist_list_input_schema() -> Value {
    derived_input_schema::<ChecklistListInput>()
}

#[must_use]
pub fn checklist_write_input_schema() -> Value {
    derived_input_schema::<ChecklistWriteInput>()
}

#[must_use]
pub fn update_plan_input_schema() -> Value {
    derived_input_schema::<UpdatePlanInput>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::plan::{UpdatePlanTool, new_shared_plan_state};
    use crate::tools::schema_sanitize;
    use crate::tools::spec::ToolSpec;
    use crate::tools::todo::{
        TodoAddTool, TodoListTool, TodoUpdateTool, TodoWriteTool, new_shared_todo_list,
    };

    fn model_visible_input_schema(tool: &dyn ToolSpec) -> Value {
        let mut schema = tool.input_schema();
        schema_sanitize::sanitize(&mut schema);
        schema
    }

    const TODO_PLAN_SCHEMA_SNAPSHOT_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/harness/kernel-v2-schema-snapshots"
    );

    #[test]
    #[ignore = "bootstrap kernel-v2 todo/plan-tool schema snapshot fixtures"]
    fn dump_todo_plan_tool_schemas_for_snapshot_bootstrap() {
        let todo_list = new_shared_todo_list();
        let plan_state = new_shared_plan_state();
        let tools: [(&str, Box<dyn ToolSpec>); 9] = [
            (
                "checklist_add",
                Box::new(TodoAddTool::checklist(todo_list.clone())),
            ),
            ("todo_add", Box::new(TodoAddTool::new(todo_list.clone()))),
            (
                "checklist_update",
                Box::new(TodoUpdateTool::checklist(
                    todo_list.clone(),
                    plan_state.clone(),
                )),
            ),
            (
                "todo_update",
                Box::new(TodoUpdateTool::new(todo_list.clone(), plan_state.clone())),
            ),
            (
                "checklist_list",
                Box::new(TodoListTool::checklist(todo_list.clone())),
            ),
            ("todo_list", Box::new(TodoListTool::new(todo_list.clone()))),
            (
                "checklist_write",
                Box::new(TodoWriteTool::checklist(
                    todo_list.clone(),
                    plan_state.clone(),
                )),
            ),
            (
                "todo_write",
                Box::new(TodoWriteTool::new(todo_list.clone(), plan_state.clone())),
            ),
            (
                "update_plan",
                Box::new(UpdatePlanTool::new(plan_state, todo_list)),
            ),
        ];
        for (name, tool) in tools {
            let schema = model_visible_input_schema(tool.as_ref());
            let pretty = serde_json::to_string_pretty(&schema).expect("serialize");
            println!("=== {name} ===\n{pretty}\n");
        }
    }

    #[test]
    fn todo_plan_tool_model_visible_schemas_match_snapshots() {
        let todo_list = new_shared_todo_list();
        let plan_state = new_shared_plan_state();
        let cases: [(&str, Box<dyn ToolSpec>, &str); 9] = [
            (
                "checklist_add",
                Box::new(TodoAddTool::checklist(todo_list.clone())),
                "checklist_add",
            ),
            (
                "todo_add",
                Box::new(TodoAddTool::new(todo_list.clone())),
                "checklist_add",
            ),
            (
                "checklist_update",
                Box::new(TodoUpdateTool::checklist(
                    todo_list.clone(),
                    plan_state.clone(),
                )),
                "checklist_update",
            ),
            (
                "todo_update",
                Box::new(TodoUpdateTool::new(todo_list.clone(), plan_state.clone())),
                "checklist_update",
            ),
            (
                "checklist_list",
                Box::new(TodoListTool::checklist(todo_list.clone())),
                "checklist_list",
            ),
            (
                "todo_list",
                Box::new(TodoListTool::new(todo_list.clone())),
                "checklist_list",
            ),
            (
                "checklist_write",
                Box::new(TodoWriteTool::checklist(
                    todo_list.clone(),
                    plan_state.clone(),
                )),
                "checklist_write",
            ),
            (
                "todo_write",
                Box::new(TodoWriteTool::new(todo_list.clone(), plan_state.clone())),
                "checklist_write",
            ),
            (
                "update_plan",
                Box::new(UpdatePlanTool::new(plan_state, todo_list)),
                "update_plan",
            ),
        ];
        for (tool_name, tool, snapshot_name) in cases {
            assert_eq!(tool.name(), tool_name);
            let schema = model_visible_input_schema(tool.as_ref());
            let path = format!("{TODO_PLAN_SCHEMA_SNAPSHOT_DIR}/todo-{snapshot_name}.json");
            let expected: Value = serde_json::from_str(
                &std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("missing snapshot {path}: {e}")),
            )
            .expect("parse snapshot JSON");
            assert_eq!(
                schema, expected,
                "model-visible schema drift for {tool_name} (snapshot {snapshot_name}) — update fixture only after explicit KV-cache review"
            );
        }
    }
}
