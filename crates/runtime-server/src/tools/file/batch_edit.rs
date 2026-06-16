//! batch_edit — apply the same search/replace across many files matched by glob.

use super::glob_targets::resolve_glob_targets;
use super::schemas::batch_edit_input_schema;
use super::search_replace::{SearchReplaceError, SearchReplaceMode, apply_search_replace};
use super::write::{atomic_write, encode_text, read_decoded_for_edit};
use super::{DIFF_MAX_INPUT_BYTES, MAX_BATCH_DIFF_BYTES, MAX_BATCH_FILES};
use crate::tools::diff_format::make_unified_diff;
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    lsp_diagnostics_for_paths, optional_bool, optional_str, required_str,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::PathBuf;

pub struct BatchEditTool;

#[derive(Debug)]
struct FileEditPlan {
    rel_path: String,
    abs_path: PathBuf,
    before: String,
    after: String,
    match_count: usize,
    enc_label: String,
    had_bom: bool,
}

fn parse_replace_mode(input: &Value) -> Result<SearchReplaceMode, ToolError> {
    match optional_str(input, "replace_mode") {
        None => Ok(SearchReplaceMode::All),
        Some("first") => Ok(SearchReplaceMode::First),
        Some("all") => Ok(SearchReplaceMode::All),
        Some(other) => Err(ToolError::invalid_input(format!(
            "replace_mode must be 'first' or 'all', not '{other}'"
        ))),
    }
}

fn per_file_diff_summary(display: &str, before: &str, after: &str) -> String {
    if before.len() > DIFF_MAX_INPUT_BYTES || after.len() > DIFF_MAX_INPUT_BYTES {
        format!(
            "(diff omitted — file exceeds {DIFF_MAX_INPUT_BYTES} bytes; \
             {before_len} -> {after_len} bytes)\n",
            before_len = before.len(),
            after_len = after.len()
        )
    } else {
        make_unified_diff(display, before, after)
    }
}

fn search_replace_error_message(path: &str, err: SearchReplaceError) -> String {
    match err {
        SearchReplaceError::EmptySearch => format!("{path}: search must not be empty"),
        SearchReplaceError::NotFound => format!("{path}: [NOT_FOUND] search string not found"),
        SearchReplaceError::Ambiguous {
            count,
            sample_lines,
        } => {
            let lines: Vec<String> = sample_lines.iter().map(|n| format!("line {n}")).collect();
            format!(
                "{path}: [AMBIGUOUS] search matched {count} times ({}) — use replace_mode:'first' or 'all'",
                lines.join(", ")
            )
        }
    }
}

fn write_file_plan(plan: &FileEditPlan) -> Result<(), ToolError> {
    let encoded = encode_text(&plan.after, &plan.enc_label, plan.had_bom);
    atomic_write(&plan.abs_path, &encoded).map_err(|e| {
        ToolError::execution_failed(format!("Failed to write {}: {e}", plan.abs_path.display()))
    })
}

#[async_trait]
impl ToolSpec for BatchEditTool {
    fn name(&self) -> &'static str {
        "batch_edit"
    }

    fn description(&self) -> &'static str {
        "Apply the same search/replace across multiple files matched by glob. \
         Defaults to dry_run:true — preview per-file diffs, then re-run with dry_run:false to apply. \
         Max 32 files per call; per-file errors do not roll back earlier writes."
    }

    fn input_schema(&self) -> Value {
        batch_edit_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::WritesFiles,
            ToolCapability::Sandboxable,
            ToolCapability::RequiresApproval,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Suggest
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let search = required_str(&input, "search")?;
        let replace = required_str(&input, "replace")?;
        let glob = required_str(&input, "glob")?;
        let base_path = optional_str(&input, "path").unwrap_or(".");
        let dry_run = optional_bool(&input, "dry_run", true);
        let respect_gitignore = optional_bool(&input, "respect_gitignore", true);
        let replace_mode = parse_replace_mode(&input)?;

        let targets = resolve_glob_targets(context, glob, base_path, respect_gitignore)?;
        if targets.is_empty() {
            return Ok(ToolResult::error(format!(
                "[NO_MATCH] glob '{glob}' under '{base_path}' matched 0 files"
            )));
        }
        if targets.len() > MAX_BATCH_FILES {
            return Err(ToolError::invalid_input(format!(
                "glob matched {} files (max {MAX_BATCH_FILES}). Narrow the pattern.",
                targets.len()
            )));
        }

        let mut plans: Vec<FileEditPlan> = Vec::new();
        let mut errors: Vec<String> = Vec::new();
        let mut diff_bytes = 0usize;

        for (abs_path, rel_path) in &targets {
            let decoded = match read_decoded_for_edit(abs_path) {
                Ok(d) => d,
                Err(e) => {
                    errors.push(format!("{rel_path}: {e}"));
                    continue;
                }
            };
            match apply_search_replace(&decoded.text, search, replace, Some(replace_mode), true) {
                Ok(outcome) => {
                    if outcome.updated == decoded.text {
                        continue;
                    }
                    let diff_len = outcome.updated.len().saturating_add(decoded.text.len());
                    diff_bytes = diff_bytes.saturating_add(diff_len);
                    if diff_bytes > MAX_BATCH_DIFF_BYTES {
                        return Err(ToolError::invalid_input(format!(
                            "total diff would exceed {MAX_BATCH_DIFF_BYTES} bytes — narrow glob or split into smaller batches"
                        )));
                    }
                    plans.push(FileEditPlan {
                        rel_path: rel_path.clone(),
                        abs_path: abs_path.clone(),
                        before: decoded.text,
                        after: outcome.updated,
                        match_count: outcome.match_count,
                        enc_label: decoded.label,
                        had_bom: decoded.had_bom,
                    });
                }
                Err(err) => match err {
                    SearchReplaceError::NotFound => {}
                    other => errors.push(search_replace_error_message(rel_path, other)),
                },
            }
        }

        if plans.is_empty() && errors.is_empty() {
            return Ok(ToolResult::success(format!(
                "[NO_CHANGE] glob '{glob}' matched {} file(s) but none contained the search string",
                targets.len()
            )));
        }

        let mut body = String::new();
        if dry_run {
            body.push_str(&format!(
                "[DRY_RUN] Would edit {} file(s) (glob '{glob}' under '{base_path}'):\n",
                plans.len()
            ));
        } else {
            body.push_str(&format!(
                "Edited {} file(s) (glob '{glob}' under '{base_path}'):\n",
                plans.len()
            ));
        }

        let mut written_paths: Vec<PathBuf> = Vec::new();
        for plan in &plans {
            body.push_str(&format!(
                "\n--- {} ({} occurrence(s)) ---\n",
                plan.rel_path, plan.match_count
            ));
            body.push_str(&per_file_diff_summary(
                &plan.rel_path,
                &plan.before,
                &plan.after,
            ));
            if !dry_run {
                if let Err(e) = write_file_plan(plan) {
                    errors.push(format!("{}: {e}", plan.rel_path));
                } else {
                    written_paths.push(plan.abs_path.clone());
                }
            }
        }

        if !errors.is_empty() {
            body.push_str("\n\nERRORS:\n");
            for err in &errors {
                body.push_str(err);
                body.push('\n');
            }
        }

        if dry_run {
            body.push_str("\nTo apply these edits, re-run with dry_run: false.");
        }

        let diag_block = if dry_run {
            String::new()
        } else {
            lsp_diagnostics_for_paths(context, &written_paths).await
        };
        if !diag_block.is_empty() {
            body.push('\n');
            body.push_str(&diag_block);
        }

        let success = errors.is_empty();
        Ok(ToolResult {
            content: body,
            success,
            metadata: Some(json!({
                "dry_run": dry_run,
                "glob": glob,
                "path": base_path,
                "matched_files": targets.len(),
                "planned_edits": plans.len(),
                "applied_edits": if dry_run { 0 } else { written_paths.len() },
                "errors": errors.len(),
                "files": plans.iter().map(|p| &p.rel_path).collect::<Vec<_>>(),
            })),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::spec::ToolContext;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn seed_import_fixture(root: &Path, count: usize) {
        for i in 0..count {
            let dir = root.join(format!("pkg/module_{i}"));
            fs::create_dir_all(&dir).expect("mkdir");
            fs::write(
                dir.join("index.ts"),
                format!("import x from '../../shared/old';\nexport {{ x as m{i} }};\n"),
            )
            .expect("write");
        }
    }

    #[tokio::test]
    async fn batch_edit_dry_run_previews_seventeen_files_without_writing() {
        let tmp = tempdir().expect("tempdir");
        seed_import_fixture(tmp.path(), 17);
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = BatchEditTool;

        let result = tool
            .execute(
                json!({
                    "search": "../../shared/old",
                    "replace": "../../../shared/new",
                    "glob": "**/index.ts",
                    "dry_run": true
                }),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success);
        assert!(result.content.contains("[DRY_RUN]"));
        assert!(result.content.contains("17 file(s)"));
        let meta = result.metadata.expect("metadata");
        assert_eq!(meta["planned_edits"], 17);
        assert_eq!(meta["applied_edits"], 0);

        for i in 0..17 {
            let text = fs::read_to_string(tmp.path().join(format!("pkg/module_{i}/index.ts")))
                .expect("read");
            assert!(
                text.contains("../../shared/old"),
                "dry_run must not write pkg/module_{i}"
            );
        }
    }

    #[tokio::test]
    async fn batch_edit_apply_updates_all_matching_files() {
        let tmp = tempdir().expect("tempdir");
        seed_import_fixture(tmp.path(), 17);
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = BatchEditTool;

        let result = tool
            .execute(
                json!({
                    "search": "../../shared/old",
                    "replace": "../../../shared/new",
                    "glob": "**/index.ts",
                    "dry_run": false
                }),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success);
        let meta = result.metadata.expect("metadata");
        assert_eq!(meta["applied_edits"], 17);

        for i in 0..17 {
            let text = fs::read_to_string(tmp.path().join(format!("pkg/module_{i}/index.ts")))
                .expect("read");
            assert!(text.contains("../../../shared/new"));
            assert!(!text.contains("../../shared/old"));
        }
    }

    #[tokio::test]
    async fn batch_edit_dry_run_surfaces_no_match_without_write() {
        let tmp = tempdir().expect("tempdir");
        fs::write(tmp.path().join("only.ts"), "const ok = 1;\n").expect("write");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = BatchEditTool;

        let result = tool
            .execute(
                json!({
                    "search": "DOES_NOT_EXIST",
                    "replace": "x",
                    "glob": "only.ts",
                    "dry_run": true
                }),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.content.contains("[NO_CHANGE]"));
        let text = fs::read_to_string(tmp.path().join("only.ts")).expect("read");
        assert_eq!(text, "const ok = 1;\n");
    }
}
