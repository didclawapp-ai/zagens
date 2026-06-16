//! `restore_file` — restore one workspace file from git HEAD (TS-08 MVP).

use super::path_input::required_path_field;
use super::schemas::restore_file_input_schema;
use crate::tools::git::git_pathspec_arg;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::Path;

use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
};

const GIT_TIMEOUT_MS: u64 = 30_000;

pub struct RestoreFileTool;

fn workspace_relative_pathspec(workspace: &Path, resolved: &Path) -> Result<String, ToolError> {
    let workspace_canon = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    let resolved_canon = resolved
        .canonicalize()
        .unwrap_or_else(|_| resolved.to_path_buf());
    let rel = resolved_canon.strip_prefix(&workspace_canon).map_err(|_| {
        ToolError::invalid_input(format!(
            "Path escapes workspace: {}",
            resolved_canon.display()
        ))
    })?;
    Ok(git_pathspec_arg(rel))
}

async fn git_in_workspace(
    workspace: &Path,
    args: &[&str],
) -> Result<std::process::Output, ToolError> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.current_dir(workspace)
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ToolError::not_available("git is not installed or not in PATH")
        } else {
            ToolError::execution_failed(format!("Failed to run git: {e}"))
        }
    })?;
    match tokio::time::timeout(
        std::time::Duration::from_millis(GIT_TIMEOUT_MS),
        child.wait_with_output(),
    )
    .await
    {
        Ok(res) => res.map_err(|e| ToolError::execution_failed(format!("Failed to run git: {e}"))),
        Err(_) => Err(ToolError::execution_failed(format!(
            "git command exceeded the {GIT_TIMEOUT_MS} ms timeout and was killed"
        ))),
    }
}

async fn assert_git_repo(workspace: &Path) -> Result<(), ToolError> {
    if !workspace.join(".git").exists() {
        return Err(ToolError::invalid_input(
            "Workspace is not a git repository; use revert_turn for snapshot rollback instead",
        ));
    }
    let out = git_in_workspace(workspace, &["rev-parse", "--is-inside-work-tree"]).await?;
    if !out.status.success() {
        return Err(ToolError::invalid_input(
            "Workspace is not a git repository; use revert_turn for snapshot rollback instead",
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if !text.trim().eq_ignore_ascii_case("true") {
        return Err(ToolError::invalid_input(
            "Workspace is not a git repository; use revert_turn for snapshot rollback instead",
        ));
    }
    Ok(())
}

#[async_trait]
impl ToolSpec for RestoreFileTool {
    fn name(&self) -> &'static str {
        "restore_file"
    }

    fn description(&self) -> &'static str {
        "Restore a single file in the workspace to its last committed version (git HEAD). \
         Use when the user asks to undo changes to one file. Requires the file to be tracked by git. \
         For rolling back an entire turn's edits, use revert_turn instead."
    }

    fn input_schema(&self) -> Value {
        restore_file_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::WritesFiles,
            ToolCapability::RequiresApproval,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let raw_path = required_path_field(&input, "restore_file")?;
        let resolved = context.resolve_path(raw_path)?;
        let pathspec = workspace_relative_pathspec(&context.workspace, &resolved)?;

        assert_git_repo(&context.workspace).await?;

        let output = git_in_workspace(
            &context.workspace,
            &["restore", "--source=HEAD", "--", &pathspec],
        )
        .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let hint = if stderr.contains("did not match any file") {
                " File may be untracked — git restore only works on committed paths."
            } else {
                ""
            };
            return Ok(ToolResult::error(format!(
                "git restore failed for '{pathspec}': {stderr}.{hint}"
            )));
        }

        Ok(ToolResult::success(format!(
            "Restored '{pathspec}' from HEAD (last committed version)."
        ))
        .with_metadata(json!({
            "path": pathspec,
            "source": "HEAD",
            "command": format!("git restore --source=HEAD -- {pathspec}"),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(root)
            .args(args)
            .status()
            .expect("git");
        assert!(status.success(), "git {:?} failed", args);
    }

    fn init_repo(root: &Path) {
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "Test"]);
        git(root, &["config", "core.autocrlf", "false"]);
    }

    #[tokio::test]
    async fn restore_file_reverts_uncommitted_changes() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        init_repo(root);
        fs::write(root.join("app.ts"), "original\n").expect("write");
        git(root, &["add", "app.ts"]);
        git(root, &["commit", "-q", "-m", "init"]);
        fs::write(root.join("app.ts"), "broken\n").expect("write");

        let tool = RestoreFileTool;
        let ctx = ToolContext::new(root.to_path_buf());
        let result = tool
            .execute(json!({ "path": "app.ts" }), &ctx)
            .await
            .expect("execute");
        assert!(result.success, "{}", result.content);
        let text = fs::read_to_string(root.join("app.ts")).expect("read");
        assert_eq!(text, "original\n");
    }

    #[tokio::test]
    async fn restore_file_rejects_non_git_workspace() {
        let tmp = tempdir().expect("tempdir");
        fs::write(tmp.path().join("x.txt"), "a").expect("write");
        let tool = RestoreFileTool;
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let result = tool.execute(json!({ "path": "x.txt" }), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn restore_file_requires_path_field() {
        let tmp = tempdir().expect("tempdir");
        let tool = RestoreFileTool;
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let err = tool.execute(json!({}), &ctx).await.unwrap_err().to_string();
        assert!(err.contains("'path'"), "{err}");
    }
}
