//! Model-visible automation tools over `AutomationManager`.

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::automation_manager::AutomationStatus;
use crate::tools::automation_inputs::{
    automation_create_input_schema, automation_id_input_schema, automation_list_input_schema,
    automation_update_input_schema,
};
use crate::tools::spec::{
    ApprovalRequirement, ToolAutomationHost, ToolCapability, ToolContext, ToolError, ToolResult,
    ToolSpec, optional_str, optional_u64, required_str,
};

pub struct AutomationCreateTool;
pub struct AutomationListTool;
pub struct AutomationReadTool;
pub struct AutomationUpdateTool;
pub struct AutomationPauseTool;
pub struct AutomationResumeTool;
pub struct AutomationDeleteTool;
pub struct AutomationRunTool;

fn require_automation_host(
    context: &ToolContext,
) -> Result<&std::sync::Arc<dyn ToolAutomationHost>, ToolError> {
    context
        .runtime
        .automation_host
        .as_ref()
        .ok_or_else(|| ToolError::not_available("AutomationManager is not attached"))
}

#[async_trait]
impl ToolSpec for AutomationCreateTool {
    fn name(&self) -> &'static str {
        "automation_create"
    }

    fn description(&self) -> &'static str {
        "Create a durable scheduled automation. Creation requires approval and recurrence is constrained to supported MINUTELY/HOURLY/DAILY/WEEKLY/MONTHLY/ONCE RRULE forms. Runs enqueue normal durable tasks."
    }

    fn input_schema(&self) -> Value {
        automation_create_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_automation_host(context)?;
        let req = json!({
            "name": required_str(&input, "name")?,
            "prompt": required_str(&input, "prompt")?,
            "rrule": required_str(&input, "rrule")?,
            "cwds": string_array(&input, "cwds")?.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
            "trigger_kind": optional_str(&input, "trigger_kind").unwrap_or("prompt"),
            "model": optional_str(&input, "model").map(ToString::to_string),
            "mode": optional_str(&input, "mode").map(ToString::to_string),
            "allow_shell": input.get("allow_shell").and_then(Value::as_bool),
            "trust_mode": context.trust_mode,
            "auto_approve": input.get("auto_approve").and_then(Value::as_bool),
            "status": if input.get("paused").and_then(Value::as_bool).unwrap_or(false) {
                AutomationStatus::Paused
            } else {
                AutomationStatus::Active
            },
        });
        let automation = host
            .create_automation(req)
            .await
            .map_err(ToolError::execution_failed)?;
        ToolResult::json(&automation).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

#[async_trait]
impl ToolSpec for AutomationListTool {
    fn name(&self) -> &'static str {
        "automation_list"
    }

    fn description(&self) -> &'static str {
        "List durable automations with status, next run, and last run timestamps."
    }

    fn input_schema(&self) -> Value {
        automation_list_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_automation_host(context)?;
        let mut automations: Vec<Value> = serde_json::from_value(
            host.list_automations()
                .await
                .map_err(ToolError::execution_failed)?,
        )
        .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        automations.truncate(optional_u64(&input, "limit", 50).clamp(1, 100) as usize);
        ToolResult::json(&automations).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

#[async_trait]
impl ToolSpec for AutomationReadTool {
    fn name(&self) -> &'static str {
        "automation_read"
    }

    fn description(&self) -> &'static str {
        "Read one durable automation plus recent run records."
    }

    fn input_schema(&self) -> Value {
        automation_id_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_automation_host(context)?;
        let id = required_str(&input, "automation_id")?;
        let automation = host
            .get_automation(id)
            .await
            .map_err(ToolError::execution_failed)?;
        let runs = host
            .list_runs(id, Some(20))
            .await
            .map_err(ToolError::execution_failed)?;
        ToolResult::json(&json!({ "automation": automation, "recent_runs": runs }))
            .map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

#[async_trait]
impl ToolSpec for AutomationUpdateTool {
    fn name(&self) -> &'static str {
        "automation_update"
    }

    fn description(&self) -> &'static str {
        "Update a durable automation. Requires approval; recurrence remains constrained to supported RRULE forms."
    }

    fn input_schema(&self) -> Value {
        automation_update_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_automation_host(context)?;
        let status = optional_str(&input, "status").map(|value| match value {
            "paused" => AutomationStatus::Paused,
            _ => AutomationStatus::Active,
        });
        let req = json!({
            "name": optional_str(&input, "name").map(ToString::to_string),
            "prompt": optional_str(&input, "prompt").map(ToString::to_string),
            "rrule": optional_str(&input, "rrule").map(ToString::to_string),
            "cwds": if input.get("cwds").is_some() {
                Some(string_array(&input, "cwds")?.into_iter().map(PathBuf::from).collect::<Vec<_>>())
            } else {
                None::<Vec<PathBuf>>
            },
            "trigger_kind": optional_str(&input, "trigger_kind").map(|value| match value {
                "task" => "task",
                _ => "prompt",
            }),
            "model": optional_str(&input, "model").map(ToString::to_string),
            "mode": optional_str(&input, "mode").map(ToString::to_string),
            "allow_shell": input.get("allow_shell").and_then(Value::as_bool),
            "auto_approve": input.get("auto_approve").and_then(Value::as_bool),
            "status": status,
        });
        let automation = host
            .update_automation(required_str(&input, "automation_id")?, req)
            .await
            .map_err(ToolError::execution_failed)?;
        ToolResult::json(&automation).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

macro_rules! write_automation_tool {
    ($ty:ident, $name:literal, $desc:literal, $method:ident) => {
        #[async_trait]
        impl ToolSpec for $ty {
            fn name(&self) -> &'static str {
                $name
            }
            fn description(&self) -> &'static str {
                $desc
            }
            fn input_schema(&self) -> Value {
                automation_id_input_schema()
            }
            fn capabilities(&self) -> Vec<ToolCapability> {
                vec![ToolCapability::RequiresApproval]
            }
            fn approval_requirement(&self) -> ApprovalRequirement {
                ApprovalRequirement::Required
            }
            async fn execute(
                &self,
                input: Value,
                context: &ToolContext,
            ) -> Result<ToolResult, ToolError> {
                let host = require_automation_host(context)?;
                let automation = host
                    .$method(required_str(&input, "automation_id")?)
                    .await
                    .map_err(|e| ToolError::execution_failed(e))?;
                ToolResult::json(&automation)
                    .map_err(|e| ToolError::execution_failed(e.to_string()))
            }
        }
    };
}

write_automation_tool!(
    AutomationPauseTool,
    "automation_pause",
    "Pause a durable automation. Requires approval.",
    pause_automation
);
write_automation_tool!(
    AutomationResumeTool,
    "automation_resume",
    "Resume a paused durable automation. Requires approval.",
    resume_automation
);
write_automation_tool!(
    AutomationDeleteTool,
    "automation_delete",
    "Delete a durable automation and its run history. Requires approval.",
    delete_automation
);

#[async_trait]
impl ToolSpec for AutomationRunTool {
    fn name(&self) -> &'static str {
        "automation_run"
    }

    fn description(&self) -> &'static str {
        "Run an automation now. The run enqueues a normal durable task and returns linked task/thread/turn ids as they become available."
    }

    fn input_schema(&self) -> Value {
        automation_id_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_automation_host(context)?;
        let run = host
            .run_now(required_str(&input, "automation_id")?)
            .await
            .map_err(ToolError::execution_failed)?;
        ToolResult::json(&run).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

fn string_array(input: &Value, field: &str) -> Result<Vec<String>, ToolError> {
    Ok(input
        .get(field)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default())
}
