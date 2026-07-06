//! `draft_skill` — H4 human-in-loop skill authoring (Phase 4.2).
//!
//! Writes to `.zagens/skill-drafts/<id>/` only. Does **not** install into the
//! skills catalogue; maintainer runs `zagens skill promote <id>` after review.

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::skills::draft::{self, DraftWriteOutcome};

use super::misc_inputs::draft_skill_input_schema;
use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
};

pub struct DraftSkillTool;

#[async_trait]
impl ToolSpec for DraftSkillTool {
    fn name(&self) -> &'static str {
        "draft_skill"
    }

    fn description(&self) -> &'static str {
        "Draft a new skill (SKILL.md + optional harness.toml) into `.zagens/skill-drafts/` for human review. \
         Does not install or activate the skill — after review, a maintainer runs `zagens skill promote <id>`. \
         Use when creating harness-backed workflows; validate gates with `zagens gate validate` patterns."
    }

    fn input_schema(&self) -> Value {
        draft_skill_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::WritesFiles]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        // Staging only; promotion requires explicit CLI (human-in-loop).
        ApprovalRequirement::Auto
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let name = input
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::missing_field("name"))?
            .trim();
        let description = input
            .get("description")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::missing_field("description"))?
            .trim();
        let body = input
            .get("body")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::missing_field("body"))?
            .trim();
        let harness_toml = input
            .get("harness_toml")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let replace = input
            .get("replace")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let outcome = draft::write_draft(
            &context.workspace,
            name,
            description,
            body,
            harness_toml,
            replace,
        )
        .map_err(|e| ToolError::execution_failed(e.to_string()))?;

        Ok(format_result(&outcome))
    }
}

fn format_result(outcome: &DraftWriteOutcome) -> ToolResult {
    let mut lines = vec![
        format!("Draft skill `{}` written.", outcome.name),
        format!("Directory: `{}`", outcome.draft_dir.display()),
        format!(
            "Next: maintainer reviews files, then `zagens skill promote {}`.",
            outcome.name
        ),
    ];
    if outcome.harness_valid {
        lines.push("harness.toml: validated OK.".into());
    }
    for warn in &outcome.harness_warnings {
        lines.push(format!("harness warning: {warn}"));
    }
    ToolResult::success(lines.join("\n")).with_metadata(json!({
        "skill_name": outcome.name,
        "draft_dir": outcome.draft_dir.display().to_string(),
        "harness_valid": outcome.harness_valid,
        "harness_warnings": outcome.harness_warnings,
        "promote_command": format!("zagens skill promote {}", outcome.name),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn execute_writes_draft() {
        let dir = tempdir().unwrap();
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let tool = DraftSkillTool;
        let result = tool
            .execute(
                json!({
                    "name": "my-draft",
                    "description": "Test draft",
                    "body": "# Workflow\nDo steps.\n"
                }),
                &ctx,
            )
            .await
            .expect("draft_skill");
        assert!(result.success);
        assert!(
            dir.path()
                .join(".zagens/skill-drafts/my-draft/SKILL.md")
                .is_file()
        );
    }
}
