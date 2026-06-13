//! schemars input types for the scratchpad tool family (kernel-v2 M2).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::tools::tool_schema::derived_input_schema;

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct ScratchpadInitAreaInput {
    pub id: String,
    pub path: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
enum ScratchpadInitTemplateInput {
    #[serde(rename = "workspace_audit")]
    WorkspaceAudit,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct ScratchpadInitInput {
    #[schemars(
        description = "Scratchpad run directory name. Defaults to active thread_id or task_id (creates the directory if missing)."
    )]
    pub run_id: Option<String>,
    #[schemars(description = "Optional human-readable audit scope stored in inventory.json")]
    pub scope: Option<String>,
    #[schemars(description = "Inventory rows (default: one pending area for workspace root)")]
    pub areas: Option<Vec<ScratchpadInitAreaInput>>,
    #[schemars(
        description = "When set to workspace_audit, auto-build inventory from workspace Cargo.toml members (includes runtime-server, desktop web-ui areas). Ignores default single-row inventory."
    )]
    pub template: Option<ScratchpadInitTemplateInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct ScratchpadStatusInput {
    #[schemars(
        description = "Scratchpad run directory name. Defaults to active thread_id or task_id when that directory exists."
    )]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
enum ScratchpadNoteKindInput {
    #[serde(rename = "finding")]
    Finding,
    #[serde(rename = "todo")]
    Todo,
    #[serde(rename = "cleared")]
    Cleared,
    #[serde(rename = "meta")]
    Meta,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct ScratchpadNoteLineInput {
    pub area_id: Option<String>,
    pub area: Option<String>,
    pub kind: Option<ScratchpadNoteKindInput>,
    pub severity: Option<String>,
    pub title: Option<String>,
    pub file: Option<String>,
    pub line: Option<u64>,
    pub line_end: Option<u64>,
    pub claim: Option<String>,
    pub evidence: Option<String>,
    pub status: Option<String>,
    pub source: Option<String>,
    pub supersedes: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct ScratchpadAppendInput {
    #[schemars(
        description = "Scratchpad run directory name. Defaults to active thread_id or task_id when that directory exists."
    )]
    pub run_id: Option<String>,
    #[schemars(description = "One notes.jsonl row (runtime adds id, ts).")]
    pub line: ScratchpadNoteLineInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct ScratchpadListNotesInput {
    #[schemars(
        description = "Scratchpad run directory name. Defaults to active thread_id or task_id when that directory exists."
    )]
    pub run_id: Option<String>,
    #[schemars(description = "Inventory area id to filter on")]
    pub area_id: String,
    #[schemars(
        extend("minimum" = 1, "maximum" = 100),
        description = "Max notes to return (default 20)"
    )]
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
enum ScratchpadAreaStatusInput {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "deferred")]
    Deferred,
    #[serde(rename = "pending")]
    Pending,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct ScratchpadSetAreaInput {
    #[schemars(
        description = "Scratchpad run directory name. Defaults to active thread_id or task_id when that directory exists."
    )]
    pub run_id: Option<String>,
    pub area_id: String,
    pub status: ScratchpadAreaStatusInput,
    #[schemars(description = "Optional human remark on the inventory row (not used for gates)")]
    pub notes: Option<String>,
    #[schemars(
        extend("minimum" = 0, "maximum" = 50),
        description = "Minimum notes.jsonl lines for this area_id. Default: 1 for done, 0 for deferred/pending/in_progress"
    )]
    pub require_min_notes: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct ScratchpadVerifyNoteInput {
    #[schemars(
        description = "Scratchpad run directory name. Defaults to active thread_id or task_id when that directory exists."
    )]
    pub run_id: Option<String>,
    #[schemars(description = "notes.jsonl id (e.g. note-012)")]
    pub note_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct ScratchpadImportAgentInput {
    #[schemars(description = "Scratchpad run id (defaults to active thread/task scratchpad)")]
    pub run_id: Option<String>,
    #[schemars(description = "Sub-agent id from agent_spawn")]
    pub agent_id: String,
    #[schemars(
        description = "Inventory area_id to import under. Use when the child emitted a mismatched area_id; must exist in inventory. When omitted, runtime also tries structured_findings.area_path against inventory paths."
    )]
    pub area_id: Option<String>,
    #[schemars(description = "Wait for agent completion before import (default true)")]
    pub block: Option<bool>,
    #[schemars(description = "Max wait when block=true (default 30000, max 3600000)")]
    pub timeout_ms: Option<u64>,
}

#[must_use]
pub fn scratchpad_init_input_schema() -> Value {
    derived_input_schema::<ScratchpadInitInput>()
}

#[must_use]
pub fn scratchpad_status_input_schema() -> Value {
    derived_input_schema::<ScratchpadStatusInput>()
}

#[must_use]
pub fn scratchpad_append_input_schema() -> Value {
    derived_input_schema::<ScratchpadAppendInput>()
}

#[must_use]
pub fn scratchpad_list_notes_input_schema() -> Value {
    derived_input_schema::<ScratchpadListNotesInput>()
}

#[must_use]
pub fn scratchpad_set_area_input_schema() -> Value {
    derived_input_schema::<ScratchpadSetAreaInput>()
}

#[must_use]
pub fn scratchpad_verify_note_input_schema() -> Value {
    derived_input_schema::<ScratchpadVerifyNoteInput>()
}

#[must_use]
pub fn scratchpad_import_agent_input_schema() -> Value {
    derived_input_schema::<ScratchpadImportAgentInput>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::schema_sanitize;
    use crate::tools::scratchpad::{
        ScratchpadAppendTool, ScratchpadInitTool, ScratchpadListNotesTool, ScratchpadSetAreaTool,
        ScratchpadStatusTool, ScratchpadVerifyNoteTool,
    };
    use crate::tools::scratchpad_agent::ScratchpadImportAgentTool;
    use crate::tools::spec::ToolSpec;
    use crate::tools::subagent::{SharedSubAgentManager, new_shared_subagent_manager};
    use std::time::Duration;

    fn test_subagent_manager() -> SharedSubAgentManager {
        new_shared_subagent_manager(std::env::temp_dir(), 1, Duration::from_secs(60))
    }

    fn model_visible_input_schema(tool: &dyn ToolSpec) -> Value {
        let mut schema = tool.input_schema();
        schema_sanitize::sanitize(&mut schema);
        schema
    }

    const SCRATCHPAD_SCHEMA_SNAPSHOT_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/harness/kernel-v2-schema-snapshots"
    );

    #[test]
    #[ignore = "bootstrap kernel-v2 scratchpad-tool schema snapshot fixtures"]
    fn dump_scratchpad_tool_schemas_for_snapshot_bootstrap() {
        let tools: [(&str, Box<dyn ToolSpec>); 7] = [
            ("scratchpad_init", Box::new(ScratchpadInitTool)),
            ("scratchpad_status", Box::new(ScratchpadStatusTool)),
            ("scratchpad_append", Box::new(ScratchpadAppendTool)),
            ("scratchpad_list_notes", Box::new(ScratchpadListNotesTool)),
            ("scratchpad_set_area", Box::new(ScratchpadSetAreaTool)),
            ("scratchpad_verify_note", Box::new(ScratchpadVerifyNoteTool)),
            (
                "scratchpad_import_agent",
                Box::new(ScratchpadImportAgentTool::new(test_subagent_manager())),
            ),
        ];
        for (name, tool) in tools {
            let schema = model_visible_input_schema(tool.as_ref());
            let pretty = serde_json::to_string_pretty(&schema).expect("serialize");
            println!("=== {name} ===\n{pretty}\n");
        }
    }

    #[test]
    fn scratchpad_tool_model_visible_schemas_match_snapshots() {
        let tools: [(&str, Box<dyn ToolSpec>); 7] = [
            ("scratchpad_init", Box::new(ScratchpadInitTool)),
            ("scratchpad_status", Box::new(ScratchpadStatusTool)),
            ("scratchpad_append", Box::new(ScratchpadAppendTool)),
            ("scratchpad_list_notes", Box::new(ScratchpadListNotesTool)),
            ("scratchpad_set_area", Box::new(ScratchpadSetAreaTool)),
            ("scratchpad_verify_note", Box::new(ScratchpadVerifyNoteTool)),
            (
                "scratchpad_import_agent",
                Box::new(ScratchpadImportAgentTool::new(test_subagent_manager())),
            ),
        ];
        for (name, tool) in tools {
            assert_eq!(tool.name(), name);
            let schema = model_visible_input_schema(tool.as_ref());
            let path = format!("{SCRATCHPAD_SCHEMA_SNAPSHOT_DIR}/scratchpad-{name}.json");
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
}
