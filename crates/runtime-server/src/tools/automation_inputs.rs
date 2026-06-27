//! schemars input types for the durable automation tool family (kernel-v2 M2).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::tools::tool_schema::derived_input_schema;

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
enum AutomationTriggerKindInput {
    #[serde(rename = "prompt")]
    Prompt,
    #[serde(rename = "task")]
    Task,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
enum AutomationModeInput {
    #[serde(rename = "agent")]
    Agent,
    #[serde(rename = "plan")]
    Plan,
    #[serde(rename = "yolo")]
    Yolo,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
enum AutomationStatusInput {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "paused")]
    Paused,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct AutomationCreateInput {
    pub name: String,
    pub prompt: String,
    #[schemars(
        description = "Supported: FREQ=MINUTELY;INTERVAL=N[;BYDAY=MO,TU] | FREQ=HOURLY;INTERVAL=N[;BYDAY=MO,TU] | FREQ=DAILY;BYHOUR=9;BYMINUTE=30[;INTERVAL=N] | FREQ=WEEKLY;BYDAY=MO;BYHOUR=9;BYMINUTE=30 | FREQ=MONTHLY;BYMONTHDAY=1;BYHOUR=9;BYMINUTE=30[;INTERVAL=N] | FREQ=ONCE;DTSTART=2026-06-10T09:00:00"
    )]
    pub rrule: String,
    pub cwds: Option<Vec<String>>,
    #[schemars(
        extend("default" = "prompt"),
        description = "prompt = conservative task defaults; task = use model/mode/shell/trust fields"
    )]
    pub trigger_kind: Option<AutomationTriggerKindInput>,
    pub model: Option<String>,
    pub mode: Option<AutomationModeInput>,
    pub allow_shell: Option<bool>,
    pub trust_mode: Option<bool>,
    pub auto_approve: Option<bool>,
    #[schemars(extend("default" = false))]
    pub paused: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct AutomationListInput {
    #[schemars(extend("minimum" = 1, "maximum" = 100, "default" = 50))]
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct AutomationIdInput {
    pub automation_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct AutomationUpdateInput {
    pub automation_id: String,
    pub name: Option<String>,
    pub prompt: Option<String>,
    pub rrule: Option<String>,
    pub cwds: Option<Vec<String>>,
    pub trigger_kind: Option<AutomationTriggerKindInput>,
    pub model: Option<String>,
    pub mode: Option<AutomationModeInput>,
    pub allow_shell: Option<bool>,
    pub trust_mode: Option<bool>,
    pub auto_approve: Option<bool>,
    pub status: Option<AutomationStatusInput>,
}

#[must_use]
pub fn automation_create_input_schema() -> Value {
    derived_input_schema::<AutomationCreateInput>()
}

#[must_use]
pub fn automation_list_input_schema() -> Value {
    derived_input_schema::<AutomationListInput>()
}

#[must_use]
pub fn automation_id_input_schema() -> Value {
    derived_input_schema::<AutomationIdInput>()
}

#[must_use]
pub fn automation_update_input_schema() -> Value {
    derived_input_schema::<AutomationUpdateInput>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::automation::{
        AutomationCreateTool, AutomationDeleteTool, AutomationListTool, AutomationPauseTool,
        AutomationReadTool, AutomationResumeTool, AutomationRunTool, AutomationUpdateTool,
    };
    use crate::tools::schema_sanitize;
    use crate::tools::spec::ToolSpec;

    fn model_visible_input_schema(tool: &dyn ToolSpec) -> Value {
        let mut schema = tool.input_schema();
        schema_sanitize::sanitize(&mut schema);
        schema
    }

    const AUTOMATION_SCHEMA_SNAPSHOT_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/harness/kernel-v2-schema-snapshots"
    );

    #[test]
    #[ignore = "bootstrap kernel-v2 automation-tool schema snapshot fixtures"]
    fn dump_automation_tool_schemas_for_snapshot_bootstrap() {
        let tools: [(&str, &dyn ToolSpec); 8] = [
            ("automation_create", &AutomationCreateTool),
            ("automation_list", &AutomationListTool),
            ("automation_read", &AutomationReadTool),
            ("automation_update", &AutomationUpdateTool),
            ("automation_pause", &AutomationPauseTool),
            ("automation_resume", &AutomationResumeTool),
            ("automation_delete", &AutomationDeleteTool),
            ("automation_run", &AutomationRunTool),
        ];
        for (name, tool) in tools {
            let schema = model_visible_input_schema(tool);
            let pretty = serde_json::to_string_pretty(&schema).expect("serialize");
            println!("=== {name} ===\n{pretty}\n");
        }
    }

    #[test]
    fn automation_tool_model_visible_schemas_match_snapshots() {
        let tools: [(&str, &dyn ToolSpec); 8] = [
            ("automation_create", &AutomationCreateTool),
            ("automation_list", &AutomationListTool),
            ("automation_read", &AutomationReadTool),
            ("automation_update", &AutomationUpdateTool),
            ("automation_pause", &AutomationPauseTool),
            ("automation_resume", &AutomationResumeTool),
            ("automation_delete", &AutomationDeleteTool),
            ("automation_run", &AutomationRunTool),
        ];
        for (name, tool) in tools {
            assert_eq!(tool.name(), name);
            let schema = model_visible_input_schema(tool);
            let path = format!("{AUTOMATION_SCHEMA_SNAPSHOT_DIR}/automation-{name}.json");
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
    fn create_schema_exposes_rrule() {
        let schema = automation_create_input_schema();
        assert!(schema["properties"]["rrule"].is_object());
        assert_eq!(schema["required"][0], "name");
    }
}
