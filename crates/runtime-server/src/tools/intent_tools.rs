//! Intent-level composite tools: `investigate`, `answer_from_repo`, `change_and_verify`.

use async_trait::async_trait;
use serde_json::Value;

use super::edit_and_check::EditAndCheckTool;
use super::explore_codebase::ExploreCodebaseTool;
use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
};
use zagens_tools::{EvidenceEnvelope, UncertaintyKind};

/// Machine-readable first line for intent tool results (models skim the head).
fn outcome_line(outcome: &str, extra_facts: &[(&str, &str)]) -> String {
    let mut parts = vec![format!("outcome={outcome}")];
    for (k, v) in extra_facts {
        parts.push(format!("{k}={v}"));
    }
    parts.join(" ")
}

/// Intent explore: wraps `explore_codebase` and attaches an evidence pack.
pub struct InvestigateTool;

#[async_trait]
impl ToolSpec for InvestigateTool {
    fn name(&self) -> &'static str {
        "investigate"
    }

    fn description(&self) -> &'static str {
        "Intent explore: glob → grep → bounded read in one call (preferred over chaining \
         glob_files/grep_files/read_file). Returns an evidence pack with citations — not a \
         natural-language answer. Same inputs as explore_codebase \
         (glob_pattern, grep_pattern, path, read_limit, …)."
    }

    fn input_schema(&self) -> Value {
        ExploreCodebaseTool.input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    fn is_noisy(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let mut result = ExploreCodebaseTool.execute(input, context).await?;
        let prior = result.evidence();
        let uncertainty = prior.as_ref().map(|e| e.uncertainty).unwrap_or_else(|| {
            if !result.success {
                UncertaintyKind::Partial
            } else if result.content.contains("matched 0 files")
                || result.content.contains("found no reads")
            {
                UncertaintyKind::NotFound
            } else {
                UncertaintyKind::Partial
            }
        });
        let cite_count = prior.as_ref().map(|e| e.citations.len()).unwrap_or(0);
        let outcome = if !result.success {
            "fail"
        } else if matches!(
            uncertainty,
            UncertaintyKind::NotFound | UncertaintyKind::Partial
        ) {
            "limited"
        } else {
            "ok"
        };
        let mut evidence = EvidenceEnvelope::new()
            .with_fact("intent", "investigate")
            .with_fact("composite", "explore_codebase")
            .with_fact("citation_count", cite_count.to_string())
            .with_fact("outcome", outcome)
            .with_uncertainty(uncertainty);
        if let Some(prior) = prior.as_ref() {
            evidence.merge_from(prior);
        }
        result = result.with_evidence(evidence);
        let ledger = result
            .evidence()
            .map(|e| e.format_ledger())
            .unwrap_or_default();
        let head = outcome_line(outcome, &[("citation_count", &cite_count.to_string())]);
        result.content = format!(
            "{head}\n\
             [investigate evidence pack — cite paths below; do not invent unread files]\n\
             {ledger}\n\n{}",
            result.content
        );
        Ok(result)
    }
}

/// Cite-or-refuse answer pack: wraps `investigate` and forces an answer shape.
pub struct AnswerFromRepoTool;

#[async_trait]
impl ToolSpec for AnswerFromRepoTool {
    fn name(&self) -> &'static str {
        "answer_from_repo"
    }

    fn description(&self) -> &'static str {
        "Intent answer: investigate the repo and return a cite-or-refuse evidence pack. \
         This tool does NOT produce a natural-language answer — only an evidence pack; \
         you must compose the user-facing answer yourself from the citations. \
         Read the first line: outcome=refuse|limited|ok (answer_allowed=true|false). \
         Prefer for factual codebase questions. Do not assert paths absent from citations. \
         Same inputs as investigate/explore_codebase."
    }

    fn input_schema(&self) -> Value {
        ExploreCodebaseTool.input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    fn is_noisy(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let mut result = InvestigateTool.execute(input, context).await?;
        let prior = result.evidence();
        let cite_count = prior.as_ref().map(|e| e.citations.len()).unwrap_or(0);
        // Refuse only when there are zero citations. Path-level cites with
        // uncertainty=not_found (e.g. glob hit, grep empty reads) still allow a
        // limited answer scoped to those paths (thr_3c79).
        let refuse = cite_count == 0;
        let limited = !refuse
            && matches!(
                prior.as_ref().map(|e| e.uncertainty),
                Some(UncertaintyKind::NotFound) | Some(UncertaintyKind::Partial)
            );
        let uncertainty = if refuse {
            UncertaintyKind::NotFound
        } else if limited {
            UncertaintyKind::Partial
        } else {
            prior
                .as_ref()
                .map(|e| e.uncertainty)
                .unwrap_or(UncertaintyKind::Partial)
        };
        let outcome = if refuse {
            "refuse"
        } else if limited {
            "limited"
        } else {
            "ok"
        };
        let answer_allowed = (!refuse).to_string();
        let mut evidence = EvidenceEnvelope::new()
            .with_fact("intent", "answer_from_repo")
            .with_fact("composite", "investigate")
            .with_fact("citation_count", cite_count.to_string())
            .with_fact("answer_allowed", &answer_allowed)
            .with_fact("answer_limited", limited.to_string())
            .with_fact("outcome", outcome)
            .with_uncertainty(uncertainty);
        if let Some(prior) = prior.as_ref() {
            evidence.merge_from(prior);
        }
        result = result.with_evidence(evidence);
        let ledger = result
            .evidence()
            .map(|e| e.format_ledger())
            .unwrap_or_default();
        let head = outcome_line(
            outcome,
            &[
                ("answer_allowed", answer_allowed.as_str()),
                ("citation_count", &cite_count.to_string()),
            ],
        );
        if refuse {
            result.content = format!(
                "{head}\n\
                 [answer_from_repo — REFUSE: no citations]\n\
                 {ledger}\n\n\
                 Do not invent file paths or line numbers. Broaden glob/grep or say you could not verify.\n\n{}",
                result.content
            );
            result.success = true; // structured refuse is still a successful tool call
        } else if limited {
            result.content = format!(
                "{head}\n\
                 [answer_from_repo — LIMITED: path citations only; re-read before strong claims]\n\
                 {ledger}\n\n\
                 You may name the cited paths below. Do not invent line-level facts \
                 until you read those files.\n\n{}",
                result.content
            );
        } else {
            result.content = format!(
                "{head}\n\
                 [answer_from_repo — cite-or-refuse; only assert paths listed below]\n\
                 {ledger}\n\n{}",
                result.content
            );
        }
        Ok(result)
    }
}

/// Intent edit+verify: wraps `edit_and_check` with a verify evidence stamp.
pub struct ChangeAndVerifyTool;

#[async_trait]
impl ToolSpec for ChangeAndVerifyTool {
    fn name(&self) -> &'static str {
        "change_and_verify"
    }

    fn description(&self) -> &'static str {
        "Intent change: edit_file → LSP diagnostics → optional scoped run_tests in one call. \
         Prefer over separate edit + test when verifying a change. Returns verification evidence \
         (not a prose summary). First line: outcome=ok|fail. Same inputs as edit_and_check \
         (path, search, replace, run_tests, test_args, …)."
    }

    fn input_schema(&self) -> Value {
        EditAndCheckTool.input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::WritesFiles,
            ToolCapability::ExecutesCode,
            ToolCapability::Sandboxable,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    fn is_noisy(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let mut result = EditAndCheckTool.execute(input, context).await?;
        let prior = result.evidence();
        let tests_ran = result.content.contains("## run_tests");
        let tests_failed = !result.success && tests_ran;
        let uncertainty = if tests_failed {
            UncertaintyKind::Partial
        } else if result.success {
            UncertaintyKind::None
        } else {
            UncertaintyKind::Partial
        };
        let outcome = if result.success { "ok" } else { "fail" };
        let mut evidence = EvidenceEnvelope::new()
            .with_fact("intent", "change_and_verify")
            .with_fact("composite", "edit_and_check")
            .with_fact("tests_ran", tests_ran.to_string())
            .with_fact("verified", result.success.to_string())
            .with_fact("outcome", outcome)
            .with_uncertainty(uncertainty);
        if let Some(prior) = prior.as_ref() {
            evidence.merge_from(prior);
        }
        result = result.with_evidence(evidence);
        let ledger = result
            .evidence()
            .map(|e| e.format_ledger())
            .unwrap_or_default();
        let head = outcome_line(
            outcome,
            &[
                ("verified", &result.success.to_string()),
                ("tests_ran", &tests_ran.to_string()),
            ],
        );
        result.content = format!(
            "{head}\n\
             [change_and_verify — edit/verify evidence below]\n\
             {ledger}\n\n{}",
            result.content
        );
        Ok(result)
    }
}
