//! schemars input types for the shell tool family (kernel-v2 M2).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::tools::tool_schema::derived_input_schema;

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct ExecShellInput {
    #[schemars(description = "The shell command to execute")]
    pub command: String,
    #[schemars(description = "Timeout in milliseconds (default: 120000, min: 1000, max: 600000)")]
    pub timeout_ms: Option<u64>,
    #[schemars(
        description = "Run in background and return task_id (default: false). Prefer true for commands that may run for minutes; poll with exec_shell_wait or task_shell_wait."
    )]
    pub background: Option<bool>,
    #[schemars(description = "Run interactively with terminal IO (default: false)")]
    pub interactive: Option<bool>,
    #[schemars(description = "Optional stdin data to send before waiting (non-interactive only)")]
    pub stdin: Option<String>,
    #[schemars(
        description = "Working directory for the command. Use this instead of `cd` / `Set-Location` inside the command string."
    )]
    pub cwd: Option<String>,
    #[schemars(
        description = "Allocate a pseudo-terminal for interactive programs (implies background)"
    )]
    pub tty: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct ShellWaitInput {
    #[schemars(description = "Task ID returned by exec_shell")]
    pub task_id: String,
    #[schemars(description = "Timeout in milliseconds (default: 5000)")]
    pub timeout_ms: Option<u64>,
    #[schemars(description = "Wait for completion before returning (default: true)")]
    pub wait: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct ShellInteractInput {
    #[schemars(description = "Task ID returned by exec_shell")]
    pub task_id: String,
    #[schemars(description = "Input to send to the task's stdin")]
    pub input: Option<String>,
    #[schemars(description = "Alias for input")]
    pub stdin: Option<String>,
    #[schemars(description = "Alias for input")]
    pub data: Option<String>,
    #[schemars(description = "Wait for output after sending input (default: 1000)")]
    pub timeout_ms: Option<u64>,
    #[schemars(description = "Close stdin after sending input")]
    pub close_stdin: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct ShellCancelInput {
    #[schemars(description = "Task ID returned by exec_shell or task_shell_start")]
    pub task_id: Option<String>,
    #[schemars(description = "Alias for task_id")]
    pub id: Option<String>,
    #[schemars(description = "Cancel all currently running background shell tasks")]
    pub all: Option<bool>,
}

#[must_use]
pub fn exec_shell_input_schema() -> Value {
    derived_input_schema::<ExecShellInput>()
}

#[must_use]
pub fn shell_wait_input_schema() -> Value {
    derived_input_schema::<ShellWaitInput>()
}

#[must_use]
pub fn shell_interact_input_schema() -> Value {
    derived_input_schema::<ShellInteractInput>()
}

#[must_use]
pub fn shell_cancel_input_schema() -> Value {
    derived_input_schema::<ShellCancelInput>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::schema_sanitize;
    use crate::tools::shell::{ExecShellTool, ShellCancelTool, ShellInteractTool, ShellWaitTool};
    use crate::tools::spec::ToolSpec;

    fn model_visible_input_schema(tool: &dyn ToolSpec) -> Value {
        let mut schema = tool.input_schema();
        schema_sanitize::sanitize(&mut schema);
        schema
    }

    const SHELL_SCHEMA_SNAPSHOT_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/harness/kernel-v2-schema-snapshots"
    );

    #[test]
    #[ignore = "bootstrap kernel-v2 shell-tool schema snapshot fixtures"]
    fn dump_shell_tool_schemas_for_snapshot_bootstrap() {
        let tools: [(&str, Box<dyn ToolSpec>); 4] = [
            ("exec_shell", Box::new(ExecShellTool)),
            (
                "exec_shell_wait",
                Box::new(ShellWaitTool::new("exec_shell_wait")),
            ),
            (
                "exec_shell_interact",
                Box::new(ShellInteractTool::new("exec_shell_interact")),
            ),
            ("exec_shell_cancel", Box::new(ShellCancelTool)),
        ];
        for (name, tool) in tools {
            let schema = model_visible_input_schema(tool.as_ref());
            let pretty = serde_json::to_string_pretty(&schema).expect("serialize");
            println!("=== {name} ===\n{pretty}\n");
        }
    }

    #[test]
    fn shell_tool_model_visible_schemas_match_snapshots() {
        let tools: [(&str, Box<dyn ToolSpec>); 4] = [
            ("exec_shell", Box::new(ExecShellTool)),
            (
                "exec_shell_wait",
                Box::new(ShellWaitTool::new("exec_shell_wait")),
            ),
            (
                "exec_shell_interact",
                Box::new(ShellInteractTool::new("exec_shell_interact")),
            ),
            ("exec_shell_cancel", Box::new(ShellCancelTool)),
        ];
        for (name, tool) in tools {
            assert_eq!(tool.name(), name);
            let schema = model_visible_input_schema(tool.as_ref());
            let path = format!("{SHELL_SCHEMA_SNAPSHOT_DIR}/shell-{name}.json");
            let expected: Value = serde_json::from_str(
                &std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("missing snapshot {path}: {e}")),
            )
            .expect("parse snapshot JSON");
            assert_eq!(
                schema, expected,
                "model-visible schema drift for {name} — update fixture only after explicit KV-cache review"
            );
        }
    }

    #[test]
    fn shell_wait_and_interact_aliases_share_primary_schemas() {
        let wait_primary = model_visible_input_schema(&ShellWaitTool::new("exec_shell_wait"));
        let wait_alias = model_visible_input_schema(&ShellWaitTool::new("exec_wait"));
        assert_eq!(wait_primary, wait_alias);

        let interact_primary =
            model_visible_input_schema(&ShellInteractTool::new("exec_shell_interact"));
        let interact_alias = model_visible_input_schema(&ShellInteractTool::new("exec_interact"));
        assert_eq!(interact_primary, interact_alias);
    }
}
