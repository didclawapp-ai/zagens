//! Model-visible automation tools over `AutomationManager`.

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::automation_manager::AutomationStatus;
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
        "Create a durable scheduled automation. Creation requires approval and recurrence is constrained to supported HOURLY/WEEKLY RRULE forms. Runs enqueue normal durable tasks."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "prompt": { "type": "string" },
                "rrule": {
                    "type": "string",
                    "description": "Supported: FREQ=HOURLY;INTERVAL=N[;BYDAY=MO,TU] or FREQ=WEEKLY;BYDAY=MO;BYHOUR=9;BYMINUTE=30"
                },
                "cwds": { "type": "array", "items": { "type": "string" } },
                "paused": { "type": "boolean", "default": false }
            },
            "required": ["name", "prompt", "rrule"],
            "additionalProperties": false
        })
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
            "status": if input.get("paused").and_then(Value::as_bool).unwrap_or(false) {
                AutomationStatus::Paused
            } else {
                AutomationStatus::Active
            },
        });
        let automation = host
            .create_automation(req)
            .await
            .map_err(|e| ToolError::execution_failed(e))?;
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
        json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 50 }
            },
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_automation_host(context)?;
        let mut automations: Vec<Value> = serde_json::from_value(
            host.list_automations()
                .await
                .map_err(|e| ToolError::execution_failed(e))?,
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
        automation_id_schema(true)
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
            .map_err(|e| ToolError::execution_failed(e))?;
        let runs = host
            .list_runs(id, Some(20))
            .await
            .map_err(|e| ToolError::execution_failed(e))?;
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
        json!({
            "type": "object",
            "properties": {
                "automation_id": { "type": "string" },
                "name": { "type": "string" },
                "prompt": { "type": "string" },
                "rrule": { "type": "string" },
                "cwds": { "type": "array", "items": { "type": "string" } },
                "status": { "type": "string", "enum": ["active", "paused"] }
            },
            "required": ["automation_id"],
            "additionalProperties": false
        })
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
            "status": status,
        });
        let automation = host
            .update_automation(required_str(&input, "automation_id")?, req)
            .await
            .map_err(|e| ToolError::execution_failed(e))?;
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
                automation_id_schema(true)
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
        automation_id_schema(true)
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
            .map_err(|e| ToolError::execution_failed(e))?;
        ToolResult::json(&run).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

fn automation_id_schema(require_id: bool) -> Value {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "automation_id": { "type": "string" }
        },
        "additionalProperties": false
    });
    if require_id {
        schema["required"] = json!(["automation_id"]);
    }
    schema
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::spec::ToolSpec;

    #[test]
    fn create_schema_exposes_rrule() {
        let schema = AutomationCreateTool.input_schema();
        assert!(schema["properties"]["rrule"].is_object());
        assert_eq!(schema["required"][0], "name");
    }
}
