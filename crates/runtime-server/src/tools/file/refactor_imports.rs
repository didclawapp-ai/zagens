//! refactor_imports — remap relative import paths across many files (TS-07 codemod).

use super::glob_targets::resolve_glob_targets;
use super::import_rewrite::rewrite_imports_in_file;
use super::schemas::refactor_imports_input_schema;
use super::write::{atomic_write, encode_text, read_decoded_for_edit};
use super::{DIFF_MAX_INPUT_BYTES, MAX_BATCH_DIFF_BYTES, MAX_BATCH_FILES};
use crate::tools::diff_format::make_unified_diff;
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    lsp_diagnostics_for_paths, optional_bool, optional_str, required_str,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub struct RefactorImportsTool;

#[derive(Debug)]
struct FileRewritePlan {
    rel_path: String,
    abs_path: PathBuf,
    before: String,
    after: String,
    rewrite_count: usize,
    enc_label: String,
    had_bom: bool,
}

fn parent_dir_posix(file_rel: &str) -> String {
    Path::new(file_rel)
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_string_lossy()
        .replace('\\', "/")
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

fn write_file_plan(plan: &FileRewritePlan) -> Result<(), ToolError> {
    let encoded = encode_text(&plan.after, &plan.enc_label, plan.had_bom);
    atomic_write(&plan.abs_path, &encoded).map_err(|e| {
        ToolError::execution_failed(format!("Failed to write {}: {e}", plan.abs_path.display()))
    })
}

#[async_trait]
impl ToolSpec for RefactorImportsTool {
    fn name(&self) -> &'static str {
        "refactor_imports"
    }

    fn description(&self) -> &'static str {
        "Remap relative import paths that resolve to a workspace module. \
         Recomputes `../` depth per file (TS/JS/Go single-line imports) — use when files sit at \
         different directory depths. Defaults to dry_run:true; max 32 files. \
         For identical search/replace in every file, use batch_edit instead."
    }

    fn input_schema(&self) -> Value {
        refactor_imports_input_schema()
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
        let from_target = required_str(&input, "from_target")?;
        let to_target = required_str(&input, "to_target")?;
        let glob = required_str(&input, "glob")?;
        let base_path = optional_str(&input, "path").unwrap_or(".");
        let dry_run = optional_bool(&input, "dry_run", true);
        let respect_gitignore = optional_bool(&input, "respect_gitignore", true);

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

        let mut plans: Vec<FileRewritePlan> = Vec::new();
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
            let file_dir = parent_dir_posix(rel_path);
            let outcome = rewrite_imports_in_file(&decoded.text, &file_dir, from_target, to_target);
            if outcome.rewrites.is_empty() {
                continue;
            }
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
            plans.push(FileRewritePlan {
                rel_path: rel_path.clone(),
                abs_path: abs_path.clone(),
                before: decoded.text,
                after: outcome.updated,
                rewrite_count: outcome.rewrites.len(),
                enc_label: decoded.label,
                had_bom: decoded.had_bom,
            });
        }

        if plans.is_empty() && errors.is_empty() {
            return Ok(ToolResult::success(format!(
                "[NO_CHANGE] glob '{glob}' matched {} file(s) but none had relative imports resolving to '{from_target}'",
                targets.len()
            )));
        }

        let mut body = String::new();
        if dry_run {
            body.push_str(&format!(
                "[DRY_RUN] Would rewrite imports in {} file(s) ({from_target} -> {to_target}, glob '{glob}'):\n",
                plans.len()
            ));
        } else {
            body.push_str(&format!(
                "Rewrote imports in {} file(s) ({from_target} -> {to_target}):\n",
                plans.len()
            ));
        }

        let mut written_paths: Vec<PathBuf> = Vec::new();
        for plan in &plans {
            body.push_str(&format!(
                "\n--- {} ({} import(s)) ---\n",
                plan.rel_path, plan.rewrite_count
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
            body.push_str("\nTo apply, re-run with dry_run: false.");
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

        Ok(ToolResult {
            content: body,
            success: errors.is_empty(),
            metadata: Some(json!({
                "dry_run": dry_run,
                "from_target": from_target,
                "to_target": to_target,
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

    fn seed_same_depth_fixture(root: &Path, count: usize) {
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

    fn seed_mixed_depth_fixture(root: &Path) {
        fs::create_dir_all(root.join("pkg/a")).expect("mkdir");
        fs::create_dir_all(root.join("pkg/a/b")).expect("mkdir");
        fs::write(
            root.join("pkg/a/index.ts"),
            "import x from '../../shared/old';\n",
        )
        .expect("write");
        fs::write(
            root.join("pkg/a/b/index.ts"),
            "import x from '../../../shared/old';\n",
        )
        .expect("write");
    }

    #[tokio::test]
    async fn refactor_imports_mixed_depth_per_file_paths() {
        let tmp = tempdir().expect("tempdir");
        seed_mixed_depth_fixture(tmp.path());
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = RefactorImportsTool;

        let result = tool
            .execute(
                json!({
                    "from_target": "shared/old",
                    "to_target": "shared/new",
                    "glob": "**/*.ts",
                    "dry_run": false
                }),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success);
        let shallow = fs::read_to_string(tmp.path().join("pkg/a/index.ts")).expect("read");
        let deep = fs::read_to_string(tmp.path().join("pkg/a/b/index.ts")).expect("read");
        assert!(shallow.contains("'../../shared/new'"));
        assert!(deep.contains("'../../../shared/new'"));
        assert!(!shallow.contains("shared/old"));
        assert!(!deep.contains("shared/old"));
    }

    #[tokio::test]
    async fn refactor_imports_seventeen_files_dry_run() {
        let tmp = tempdir().expect("tempdir");
        seed_same_depth_fixture(tmp.path(), 17);
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = RefactorImportsTool;

        let result = tool
            .execute(
                json!({
                    "from_target": "shared/old",
                    "to_target": "shared/new",
                    "glob": "**/index.ts",
                    "dry_run": true
                }),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success);
        assert!(result.content.contains("[DRY_RUN]"));
        let meta = result.metadata.expect("metadata");
        assert_eq!(meta["planned_edits"], 17);

        for i in 0..17 {
            let text = fs::read_to_string(tmp.path().join(format!("pkg/module_{i}/index.ts")))
                .expect("read");
            assert!(text.contains("shared/old"));
        }
    }

    #[tokio::test]
    async fn refactor_imports_no_match_leaves_files() {
        let tmp = tempdir().expect("tempdir");
        fs::write(tmp.path().join("app.ts"), "import z from './local';\n").expect("write");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = RefactorImportsTool;

        let result = tool
            .execute(
                json!({
                    "from_target": "shared/missing",
                    "to_target": "shared/new",
                    "glob": "app.ts",
                    "dry_run": false
                }),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.content.contains("[NO_CHANGE]"));
        let text = fs::read_to_string(tmp.path().join("app.ts")).expect("read");
        assert_eq!(text, "import z from './local';\n");
    }
}
