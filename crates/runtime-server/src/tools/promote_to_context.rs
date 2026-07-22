//! `promote_to_context` — load the last workshop-routed large tool output.

use async_trait::async_trait;
use serde_json::{Value, json};

use super::large_output_router::WORKSHOP_LAST_TOOL_RESULT_VAR;
use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool, optional_u64,
};
use zagens_tools::{EvidenceEnvelope, UncertaintyKind};

/// Promote (or peek) the last large tool output stored in workshop variables.
pub struct PromoteToContextTool;

#[async_trait]
impl ToolSpec for PromoteToContextTool {
    fn name(&self) -> &'static str {
        "promote_to_context"
    }

    fn description(&self) -> &'static str {
        "Load the full raw text of the most recent large tool output that was \
         routed through the workshop (see workshop-ref / last_tool_result). \
         Use after a truncated synthesis when you need the complete body. \
         Consumes the stored blob by default; set peek=true to inspect without clearing."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "peek": {
                    "type": "boolean",
                    "description": "If true, return a status/preview without consuming the stored raw output (default false)."
                },
                "max_chars": {
                    "type": "integer",
                    "minimum": 500,
                    "maximum": 200000,
                    "description": "Cap promoted content size (default 60000). Excess is truncated with uncertainty=truncated."
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let peek = optional_bool(&input, "peek", false);
        let max_chars = optional_u64(&input, "max_chars", 60_000).clamp(500, 200_000) as usize;

        let Some(vars_arc) = context.workshop_vars.as_ref() else {
            return Ok(ToolResult::error(
                "promote_to_context: workshop variables are not enabled for this session \
                 (large-output router disabled).",
            ));
        };

        let mut vars = vars_arc.lock().await;
        if peek {
            let Some((name, raw, ext)) = vars.peek_raw() else {
                return Ok(ToolResult::success(format!(
                    "No large tool output stored in `{WORKSHOP_LAST_TOOL_RESULT_VAR}`."
                ))
                .with_evidence(
                    EvidenceEnvelope::new()
                        .with_fact("workshop_empty", "true")
                        .with_uncertainty(UncertaintyKind::NotFound),
                ));
            };
            let chars = raw.chars().count();
            let preview: String = raw.chars().take(800).collect();
            let ref_id = ext.map(|e| e.ref_id.as_str()).unwrap_or("none");
            return Ok(ToolResult::success(format!(
                "[peek] tool={name} ref_id={ref_id} chars={chars}\n{preview}"
            ))
            .with_evidence(
                EvidenceEnvelope::new()
                    .with_fact("tool", name)
                    .with_fact("chars", chars.to_string())
                    .with_fact("peek", "true")
                    .with_uncertainty(UncertaintyKind::Partial),
            ));
        }

        let Some((name, raw)) = vars.take_raw() else {
            return Ok(ToolResult::success(format!(
                "No large tool output stored in `{WORKSHOP_LAST_TOOL_RESULT_VAR}` to promote."
            ))
            .with_evidence(
                EvidenceEnvelope::new()
                    .with_fact("workshop_empty", "true")
                    .with_uncertainty(UncertaintyKind::NotFound),
            ));
        };

        let total = raw.chars().count();
        let (body, uncertainty) = if total > max_chars {
            let head: String = raw.chars().take(max_chars).collect();
            (
                format!(
                    "[promoted from workshop tool={name} chars={total} capped_at={max_chars}]\n\
                     Remaining content was dropped — raise max_chars or re-run the original tool.\n\n{head}"
                ),
                UncertaintyKind::Truncated,
            )
        } else {
            (
                format!("[promoted from workshop tool={name} chars={total}]\n\n{raw}"),
                UncertaintyKind::None,
            )
        };

        Ok(ToolResult::success(body).with_evidence(
            EvidenceEnvelope::new()
                .with_fact("promoted_tool", name)
                .with_fact("chars", total.to_string())
                .with_uncertainty(uncertainty),
        ))
    }
}
