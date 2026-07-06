//! Input schemas for miscellaneous built-in tools (kernel-v2 M2).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::tool_schema::derived_input_schema;

const REVERT_TURN_MAX_OFFSET: u64 = 50;

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct FileInfoInput {
    #[schemars(description = "Path to the file (relative to workspace or absolute)")]
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct RememberInput {
    #[schemars(description = "The single-sentence durable note to remember.")]
    pub note: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct DescribeImageInput {
    #[schemars(description = "Path to the image file. Supported: png, jpg, jpeg, gif, bmp, webp.")]
    pub path: String,
    #[schemars(description = "Optional custom prompt for the vision model.")]
    pub prompt: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct FimEditInput {
    #[schemars(description = "Path to the file to edit (relative to workspace)")]
    pub path: String,
    #[schemars(
        description = "Text anchor marking the end of the prefix. Everything up to and including this anchor is kept as-is before the generated middle."
    )]
    pub prefix_anchor: String,
    #[schemars(
        description = "Text anchor marking the start of the suffix. Everything from this anchor onward is kept as-is after the generated middle."
    )]
    pub suffix_anchor: String,
    #[schemars(description = "Maximum tokens to generate (default: 1024)")]
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct ProjectMapInput {
    #[schemars(description = "Maximum depth for the tree view (default: 3).")]
    pub max_depth: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct RecallArchiveInput {
    #[schemars(description = "Search query. Tokenized and BM25-scored against archived messages.")]
    pub query: String,
    #[schemars(description = "Optional: limit to a specific prior cycle number.")]
    pub cycle: Option<u64>,
    #[schemars(description = "Maximum hits to return (default 3, hard-capped at 10).")]
    pub max_results: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct LoadSkillInput {
    #[schemars(
        description = "Skill id (the `name` field from the SKILL.md frontmatter, also shown in the `## Skills` listing)."
    )]
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct DraftSkillInput {
    #[schemars(description = "Stable skill id (lowercase, digits, hyphens; 1–64 chars).")]
    pub name: String,
    #[schemars(description = "When-to-use summary (shown in ## Skills catalogue).")]
    pub description: String,
    #[schemars(description = "SKILL.md body markdown (frontmatter added automatically).")]
    pub body: String,
    #[schemars(
        description = "Optional HarnessContract TOML (stages + verify rows). Validated before write."
    )]
    pub harness_toml: Option<String>,
    #[schemars(description = "Overwrite an existing draft with the same id.")]
    pub replace: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct RevertTurnInput {
    #[schemars(
        extend("minimum" = 1, "maximum" = REVERT_TURN_MAX_OFFSET),
        description = "How many turns back to revert (default 1)."
    )]
    pub turn_offset: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct RunTestsInput {
    #[schemars(description = "Optional extra arguments to pass to `cargo test` (shell-style).")]
    pub args: Option<String>,
    #[schemars(description = "When true, include `--all-features`.")]
    pub all_features: Option<bool>,
    #[schemars(
        description = "Wall-clock timeout in milliseconds before the test run is killed (default 600,000; max 1,800,000)."
    )]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
enum ValidateDataFormatInput {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "toml")]
    Toml,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct ValidateDataInput {
    #[schemars(description = "Optional path to a file within the workspace.")]
    pub path: Option<String>,
    #[schemars(description = "Optional inline content to validate.")]
    pub content: Option<String>,
    #[schemars(
        extend("default" = "auto"),
        description = "Validation format. 'auto' infers from extension then falls back to trying both."
    )]
    pub format: Option<ValidateDataFormatInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct DiagnosticsInput {}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct ReviewInput {
    #[schemars(
        description = "File path, PR URL, or the literal 'diff'/'staged' for git diff review."
    )]
    pub target: String,
    #[schemars(description = "Optional explicit target type: file, diff, or pr.")]
    pub kind: Option<String>,
    #[schemars(description = "Optional git base ref when using diff target (e.g. origin/main).")]
    pub base: Option<String>,
    #[schemars(description = "Review staged changes when using diff target (default: false).")]
    pub staged: Option<bool>,
    #[schemars(description = "Maximum characters to include from the source (default: 200000).")]
    pub max_chars: Option<u64>,
}

#[must_use]
pub fn file_info_input_schema() -> Value {
    derived_input_schema::<FileInfoInput>()
}

#[must_use]
pub fn remember_input_schema() -> Value {
    derived_input_schema::<RememberInput>()
}

#[must_use]
pub fn describe_image_input_schema() -> Value {
    derived_input_schema::<DescribeImageInput>()
}

#[must_use]
pub fn fim_edit_input_schema() -> Value {
    derived_input_schema::<FimEditInput>()
}

#[must_use]
pub fn project_map_input_schema() -> Value {
    derived_input_schema::<ProjectMapInput>()
}

#[must_use]
pub fn recall_archive_input_schema() -> Value {
    derived_input_schema::<RecallArchiveInput>()
}

#[must_use]
pub fn load_skill_input_schema() -> Value {
    derived_input_schema::<LoadSkillInput>()
}

#[must_use]
pub fn draft_skill_input_schema() -> Value {
    derived_input_schema::<DraftSkillInput>()
}

#[must_use]
pub fn revert_turn_input_schema() -> Value {
    derived_input_schema::<RevertTurnInput>()
}

#[must_use]
pub fn run_tests_input_schema() -> Value {
    derived_input_schema::<RunTestsInput>()
}

#[must_use]
pub fn validate_data_input_schema() -> Value {
    derived_input_schema::<ValidateDataInput>()
}

#[must_use]
pub fn diagnostics_input_schema() -> Value {
    let mut schema = derived_input_schema::<DiagnosticsInput>();
    if schema.get("properties").is_none() {
        schema["properties"] = json!({});
    }
    schema
}

#[must_use]
pub fn review_input_schema() -> Value {
    derived_input_schema::<ReviewInput>()
}

#[must_use]
pub fn request_user_input_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "questions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "header": { "type": "string" },
                        "id": { "type": "string" },
                        "question": { "type": "string" },
                        "options": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "label": { "type": "string" },
                                    "description": { "type": "string" }
                                },
                                "required": ["label", "description"]
                            },
                            "minItems": 2,
                            "maxItems": 3
                        }
                    },
                    "required": ["header", "id", "question", "options"]
                },
                "minItems": 1,
                "maxItems": 3
            }
        },
        "required": ["questions"]
    })
}

#[must_use]
pub fn multi_tool_use_parallel_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tool_uses": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "recipient_name": { "type": "string" },
                        "parameters": { "type": "object" }
                    },
                    "required": ["recipient_name", "parameters"]
                }
            }
        },
        "required": ["tool_uses"]
    })
}

#[must_use]
pub fn apply_patch_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "Path to the file to patch (relative to workspace)"
            },
            "patch": {
                "type": "string",
                "description": "Unified diff patch content"
            },
            "changes": {
                "type": "array",
                "description": "Optional full file replacements (path + content).",
                "items": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }
            },
            "fuzz": {
                "type": "integer",
                "description": "Maximum fuzz factor for fuzzy matching (default: 3, max: 50)"
            },
            "create_if_missing": {
                "type": "boolean",
                "description": "Create the file if it doesn't exist (for new file patches)"
            }
        },
        "oneOf": [
            { "required": ["patch"] },
            { "required": ["changes"] }
        ]
    })
}

#[must_use]
pub fn rlm_input_schema() -> Value {
    json!({
        "type": "object",
        "required": ["task"],
        "properties": {
            "task": {
                "type": "string",
                "description": "What to do with the input (e.g. \"Summarize the security model\", \"Extract all API endpoints\", \"Categorize each row by sentiment\"). The sub-agent uses this as its objective."
            },
            "file_path": {
                "type": "string",
                "description": "Workspace-relative path to a file to load as PROMPT. Preferred — keeps the long input out of your context. Mutually exclusive with `content`."
            },
            "content": {
                "type": "string",
                "description": "Inline content to load as PROMPT. Use only when the input isn't a file you can point at. Capped at 200k chars."
            },
            "max_depth": {
                "type": "integer",
                "description": "Recursion budget for `sub_rlm()` calls. 0 disables recursion; default 1 matches paper experiments."
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::apply_patch::ApplyPatchTool;
    use crate::tools::describe_image::DescribeImageTool;
    use crate::tools::diagnostics::DiagnosticsTool;
    use crate::tools::file_info::FileInfoTool;
    use crate::tools::fim::FimEditTool;
    use crate::tools::parallel::MultiToolUseParallelTool;
    use crate::tools::project::ProjectMapTool;
    use crate::tools::recall_archive::RecallArchiveTool;
    use crate::tools::remember::RememberTool;
    use crate::tools::revert_turn::RevertTurnTool;
    use crate::tools::review::ReviewTool;
    use crate::tools::rlm::RlmTool;
    use crate::tools::schema_sanitize;
    use crate::tools::skill::LoadSkillTool;
    use crate::tools::spec::ToolSpec;
    use crate::tools::test_runner::RunTestsTool;
    use crate::tools::user_input::RequestUserInputTool;
    use crate::tools::validate_data::ValidateDataTool;

    fn model_visible_input_schema(tool: &dyn ToolSpec) -> Value {
        let mut schema = tool.input_schema();
        schema_sanitize::sanitize(&mut schema);
        schema
    }

    const MISC_SCHEMA_SNAPSHOT_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/harness/kernel-v2-schema-snapshots"
    );

    fn fim_tool() -> FimEditTool {
        FimEditTool::new(None, String::new())
    }

    fn review_tool() -> ReviewTool {
        ReviewTool::new(None, String::new())
    }

    fn rlm_tool() -> RlmTool {
        RlmTool::new(None, String::new())
    }

    #[test]
    #[ignore = "bootstrap kernel-v2 misc-tool schema snapshot fixtures"]
    fn dump_misc_tool_schemas_for_snapshot_bootstrap() {
        let tools: [(&str, &dyn ToolSpec); 16] = [
            ("file_info", &FileInfoTool),
            ("remember", &RememberTool),
            ("request_user_input", &RequestUserInputTool),
            ("revert_turn", &RevertTurnTool),
            ("multi_tool_use.parallel", &MultiToolUseParallelTool),
            ("project_map", &ProjectMapTool),
            ("run_tests", &RunTestsTool),
            ("recall_archive", &RecallArchiveTool),
            ("validate_data", &ValidateDataTool),
            ("fim_edit", &fim_tool()),
            ("describe_image", &DescribeImageTool),
            ("diagnostics", &DiagnosticsTool),
            ("load_skill", &LoadSkillTool),
            ("apply_patch", &ApplyPatchTool),
            ("review", &review_tool()),
            ("rlm", &rlm_tool()),
        ];
        for (name, tool) in tools {
            let schema = model_visible_input_schema(tool);
            let pretty = serde_json::to_string_pretty(&schema).expect("serialize");
            println!("=== {name} ===\n{pretty}\n");
        }
    }

    #[test]
    fn misc_tool_model_visible_schemas_match_snapshots() {
        let tools: [(&str, &dyn ToolSpec); 16] = [
            ("file_info", &FileInfoTool),
            ("remember", &RememberTool),
            ("request_user_input", &RequestUserInputTool),
            ("revert_turn", &RevertTurnTool),
            ("multi_tool_use.parallel", &MultiToolUseParallelTool),
            ("project_map", &ProjectMapTool),
            ("run_tests", &RunTestsTool),
            ("recall_archive", &RecallArchiveTool),
            ("validate_data", &ValidateDataTool),
            ("fim_edit", &fim_tool()),
            ("describe_image", &DescribeImageTool),
            ("diagnostics", &DiagnosticsTool),
            ("load_skill", &LoadSkillTool),
            ("apply_patch", &ApplyPatchTool),
            ("review", &review_tool()),
            ("rlm", &rlm_tool()),
        ];
        for (name, tool) in tools {
            assert_eq!(tool.name(), name);
            let schema = model_visible_input_schema(tool);
            let path = format!("{MISC_SCHEMA_SNAPSHOT_DIR}/misc-{name}.json");
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
