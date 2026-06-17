//! schemars input types for the GitHub tool family (kernel-v2 M2).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::tools::tool_schema::derived_input_schema;

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct GithubIssueContextInput {
    #[schemars(extend("minimum" = 1))]
    pub number: u64,
    #[schemars(extend("default" = true))]
    pub include_comments: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct GithubPrContextInput {
    #[schemars(extend("minimum" = 1))]
    pub number: u64,
    #[schemars(extend("default" = false))]
    pub include_diff: Option<bool>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
enum GithubCommentTargetInput {
    #[serde(rename = "issue")]
    Issue,
    #[serde(rename = "pr")]
    Pr,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct GithubCommentInput {
    pub target: GithubCommentTargetInput,
    #[schemars(extend("minimum" = 1))]
    pub number: u64,
    pub body: String,
    pub evidence: Value,
    #[schemars(extend("default" = false))]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct GithubCloseEvidenceInput {
    pub files_changed: Vec<String>,
    pub tests_run: Vec<String>,
    pub commits: Option<Vec<String>>,
    pub final_status: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct GithubCloseIssueInput {
    #[schemars(extend("minimum" = 1))]
    pub number: u64,
    pub acceptance_criteria: Vec<String>,
    pub evidence: GithubCloseEvidenceInput,
    pub comment: Option<String>,
    #[schemars(extend("default" = false))]
    pub allow_dirty: Option<bool>,
    #[schemars(extend("default" = false))]
    pub dry_run: Option<bool>,
}

fn patch_github_comment_evidence(schema: &mut Value) {
    if let Some(props) = schema.get_mut("properties").and_then(|v| v.as_object_mut()) {
        props.insert("evidence".into(), json!({ "type": "object" }));
    }
}

fn patch_github_close_issue_schema(schema: &mut Value) {
    if let Some(criteria) = schema.pointer_mut("/properties/acceptance_criteria")
        && let Some(obj) = criteria.as_object_mut()
    {
        obj.insert("minItems".into(), json!(1));
    }
    if let Some(evidence) = schema
        .pointer_mut("/properties/evidence")
        .and_then(|v| v.as_object_mut())
    {
        evidence.insert(
            "required".into(),
            json!(["files_changed", "tests_run", "final_status"]),
        );
        reorder_evidence_schema(evidence);
    }
}

fn reorder_evidence_schema(obj: &mut Map<String, Value>) {
    let old = std::mem::take(obj);
    for key in ["type", "properties", "required", "additionalProperties"] {
        if let Some(val) = old.get(key) {
            obj.insert(key.to_string(), val.clone());
        }
    }
    for (key, val) in old {
        if !obj.contains_key(&key) {
            obj.insert(key, val);
        }
    }
}

#[must_use]
pub fn github_issue_context_input_schema() -> Value {
    derived_input_schema::<GithubIssueContextInput>()
}

#[must_use]
pub fn github_pr_context_input_schema() -> Value {
    derived_input_schema::<GithubPrContextInput>()
}

#[must_use]
pub fn github_comment_input_schema() -> Value {
    let mut schema = derived_input_schema::<GithubCommentInput>();
    patch_github_comment_evidence(&mut schema);
    schema
}

#[must_use]
pub fn github_close_issue_input_schema() -> Value {
    let mut schema = derived_input_schema::<GithubCloseIssueInput>();
    patch_github_close_issue_schema(&mut schema);
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::github::{
        GithubCloseIssueTool, GithubCommentTool, GithubIssueContextTool, GithubPrContextTool,
    };
    use crate::tools::schema_sanitize;
    use crate::tools::spec::ToolSpec;

    fn model_visible_input_schema(tool: &dyn ToolSpec) -> Value {
        let mut schema = tool.input_schema();
        schema_sanitize::sanitize(&mut schema);
        schema
    }

    const GITHUB_SCHEMA_SNAPSHOT_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/harness/kernel-v2-schema-snapshots"
    );

    #[test]
    #[ignore = "bootstrap kernel-v2 github-tool schema snapshot fixtures"]
    fn dump_github_tool_schemas_for_snapshot_bootstrap() {
        let tools: [(&str, &dyn ToolSpec); 4] = [
            ("github_issue_context", &GithubIssueContextTool),
            ("github_pr_context", &GithubPrContextTool),
            ("github_comment", &GithubCommentTool),
            ("github_close_issue", &GithubCloseIssueTool),
        ];
        for (name, tool) in tools {
            let schema = model_visible_input_schema(tool);
            let pretty = serde_json::to_string_pretty(&schema).expect("serialize");
            println!("=== {name} ===\n{pretty}\n");
        }
    }

    #[test]
    fn github_tool_model_visible_schemas_match_snapshots() {
        let tools: [(&str, &dyn ToolSpec); 4] = [
            ("github_issue_context", &GithubIssueContextTool),
            ("github_pr_context", &GithubPrContextTool),
            ("github_comment", &GithubCommentTool),
            ("github_close_issue", &GithubCloseIssueTool),
        ];
        for (name, tool) in tools {
            assert_eq!(tool.name(), name);
            let schema = model_visible_input_schema(tool);
            let path = format!("{GITHUB_SCHEMA_SNAPSHOT_DIR}/github-{name}.json");
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
