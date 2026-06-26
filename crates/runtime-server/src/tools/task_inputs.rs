//! schemars input types for the durable task tool family (kernel-v2 M2).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::tools::tool_schema::derived_input_schema;

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
enum TaskCreateModeInput {
    #[serde(rename = "agent")]
    Agent,
    #[serde(rename = "plan")]
    Plan,
    #[serde(rename = "yolo")]
    Yolo,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct TaskCreateInput {
    #[schemars(description = "Work prompt for the durable task.")]
    pub prompt: String,
    pub model: Option<String>,
    #[schemars(description = "Workspace path; defaults to current workspace.")]
    pub workspace: Option<String>,
    pub mode: Option<TaskCreateModeInput>,
    pub allow_shell: Option<bool>,
    pub auto_approve: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct TaskListInput {
    #[schemars(extend("minimum" = 1, "maximum" = 100, "default" = 20))]
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct TaskIdInput {
    #[schemars(description = "Full task id or unambiguous prefix.")]
    pub task_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct TaskIdOptionalInput {
    #[schemars(description = "Task id; defaults to active task.")]
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
enum TaskGateKindInput {
    #[serde(rename = "fmt")]
    Fmt,
    #[serde(rename = "check")]
    Check,
    #[serde(rename = "clippy")]
    Clippy,
    #[serde(rename = "test")]
    Test,
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct TaskGateRunInput {
    #[schemars(description = "Gate category.")]
    pub gate: TaskGateKindInput,
    #[schemars(description = "Command to run.")]
    pub command: String,
    #[schemars(description = "Optional working directory within the workspace.")]
    pub cwd: Option<String>,
    #[schemars(extend("minimum" = 1000, "maximum" = 600000))]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct TaskShellStartInput {
    pub command: String,
    #[schemars(description = "Optional working directory within the workspace.")]
    pub cwd: Option<String>,
    #[schemars(extend("minimum" = 1000, "maximum" = 600000))]
    pub timeout_ms: Option<u64>,
    pub stdin: Option<String>,
    pub tty: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct TaskShellWaitInput {
    #[schemars(
        description = "Background shell task id returned by task_shell_start or exec_shell."
    )]
    pub task_id: String,
    #[schemars(extend("default" = false))]
    pub wait: Option<bool>,
    #[schemars(extend("minimum" = 1000, "maximum" = 600000))]
    pub timeout_ms: Option<u64>,
    pub gate: Option<TaskGateKindInput>,
    #[schemars(description = "Original command, used when recording gate evidence.")]
    pub command: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct PrAttemptRecordInput {
    #[schemars(description = "Task to attach to; defaults to active task.")]
    pub task_id: Option<String>,
    pub attempt_group_id: Option<String>,
    #[schemars(extend("minimum" = 1))]
    pub attempt_index: Option<u64>,
    #[schemars(extend("minimum" = 1))]
    pub attempt_count: Option<u64>,
    pub summary: String,
    pub verification: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct PrAttemptReadInput {
    #[schemars(description = "Task id; defaults to active task.")]
    pub task_id: Option<String>,
    pub attempt_id: String,
}

#[must_use]
pub fn task_create_input_schema() -> Value {
    derived_input_schema::<TaskCreateInput>()
}

#[must_use]
pub fn task_list_input_schema() -> Value {
    derived_input_schema::<TaskListInput>()
}

#[must_use]
pub fn task_read_input_schema() -> Value {
    derived_input_schema::<TaskIdInput>()
}

#[must_use]
pub fn task_cancel_input_schema() -> Value {
    derived_input_schema::<TaskIdInput>()
}

#[must_use]
pub fn task_gate_run_input_schema() -> Value {
    derived_input_schema::<TaskGateRunInput>()
}

#[must_use]
pub fn task_shell_start_input_schema() -> Value {
    derived_input_schema::<TaskShellStartInput>()
}

#[must_use]
pub fn task_shell_wait_input_schema() -> Value {
    derived_input_schema::<TaskShellWaitInput>()
}

#[must_use]
pub fn pr_attempt_record_input_schema() -> Value {
    derived_input_schema::<PrAttemptRecordInput>()
}

#[must_use]
pub fn pr_attempt_list_input_schema() -> Value {
    derived_input_schema::<TaskIdOptionalInput>()
}

#[must_use]
pub fn pr_attempt_read_input_schema() -> Value {
    derived_input_schema::<PrAttemptReadInput>()
}

#[must_use]
pub fn pr_attempt_preflight_input_schema() -> Value {
    derived_input_schema::<PrAttemptReadInput>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::schema_sanitize;
    use crate::tools::spec::ToolSpec;
    use crate::tools::tasks::{
        PrAttemptListTool, PrAttemptPreflightTool, PrAttemptReadTool, PrAttemptRecordTool,
        TaskCancelTool, TaskCreateTool, TaskGateRunTool, TaskListTool, TaskReadTool,
        TaskShellStartTool, TaskShellWaitTool,
    };

    fn model_visible_input_schema(tool: &dyn ToolSpec) -> Value {
        let mut schema = tool.input_schema();
        schema_sanitize::sanitize(&mut schema);
        schema
    }

    const TASK_SCHEMA_SNAPSHOT_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/harness/kernel-v2-schema-snapshots"
    );

    #[test]
    #[ignore = "bootstrap kernel-v2 task-tool schema snapshot fixtures"]
    fn dump_task_tool_schemas_for_snapshot_bootstrap() {
        let tools: [(&str, &dyn ToolSpec); 10] = [
            ("task_create", &TaskCreateTool),
            ("task_list", &TaskListTool),
            ("task_read", &TaskReadTool),
            ("task_cancel", &TaskCancelTool),
            ("task_gate_run", &TaskGateRunTool),
            ("task_shell_start", &TaskShellStartTool),
            ("task_shell_wait", &TaskShellWaitTool),
            ("pr_attempt_record", &PrAttemptRecordTool),
            ("pr_attempt_list", &PrAttemptListTool),
            ("pr_attempt_read", &PrAttemptReadTool),
        ];
        for (name, tool) in tools {
            let schema = model_visible_input_schema(tool);
            let pretty = serde_json::to_string_pretty(&schema).expect("serialize");
            println!("=== {name} ===\n{pretty}\n");
        }
        let preflight = model_visible_input_schema(&PrAttemptPreflightTool);
        let pretty = serde_json::to_string_pretty(&preflight).expect("serialize");
        println!("=== pr_attempt_preflight ===\n{pretty}\n");
    }

    #[test]
    fn task_tool_model_visible_schemas_match_snapshots() {
        let cases: [(&str, &dyn ToolSpec, &str); 11] = [
            ("task_create", &TaskCreateTool, "task_create"),
            ("task_list", &TaskListTool, "task_list"),
            ("task_read", &TaskReadTool, "task_read"),
            ("task_cancel", &TaskCancelTool, "task_cancel"),
            ("task_gate_run", &TaskGateRunTool, "task_gate_run"),
            ("task_shell_start", &TaskShellStartTool, "task_shell_start"),
            ("task_shell_wait", &TaskShellWaitTool, "task_shell_wait"),
            (
                "pr_attempt_record",
                &PrAttemptRecordTool,
                "pr_attempt_record",
            ),
            ("pr_attempt_list", &PrAttemptListTool, "pr_attempt_list"),
            ("pr_attempt_read", &PrAttemptReadTool, "pr_attempt_read"),
            (
                "pr_attempt_preflight",
                &PrAttemptPreflightTool,
                "pr_attempt_preflight",
            ),
        ];
        for (tool_name, tool, snapshot_name) in cases {
            assert_eq!(tool.name(), tool_name);
            let schema = model_visible_input_schema(tool);
            let path = format!("{TASK_SCHEMA_SNAPSHOT_DIR}/task-{snapshot_name}.json");
            let expected: Value = serde_json::from_str(
                &std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("missing snapshot {path}: {e}")),
            )
            .expect("parse snapshot JSON");
            assert_eq!(
                schema, expected,
                "model-visible schema drift for {tool_name} — update fixture only after explicit KV-cache review"
            );
        }
    }
}
