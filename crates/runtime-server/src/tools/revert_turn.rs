//! `revert_turn` — agent-callable tool that rewinds the workspace to a
//! prior pre-turn snapshot.
//!
//! The model invokes this when the user says something like "undo the
//! last edit" or "roll back". It mirrors `/restore` but speaks JSON and
//! takes a turn-offset (default 1 = previous turn) instead of a list
//! index, so the model doesn't have to count entries.
//!
//! Approval is `Required` because this mutates the workspace.

use async_trait::async_trait;
use serde_json::Value;
use zagens_runtime_adapters::snapshot::{
    DEFAULT_REVERT_TURN_OFFSET, MAX_REVERT_TURN_OFFSET, revert_pre_turn_offset,
};

use super::misc_inputs::revert_turn_input_schema;
use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, optional_u64,
};
use crate::snapshot::SnapshotRepo;

pub struct RevertTurnTool;

#[async_trait]
impl ToolSpec for RevertTurnTool {
    fn name(&self) -> &str {
        "revert_turn"
    }

    fn description(&self) -> &str {
        "Roll back the workspace files to the snapshot taken before a recent turn. \
         Use when the user explicitly asks to undo, revert, or roll back the most recent edits. \
         `turn_offset` is 1-based: 1 reverts the most recent turn, 2 reverts the previous one, \
         and so on (max 50). Conversation history is NOT modified — only working-tree files are \
         restored from the side-git snapshot repo."
    }

    fn input_schema(&self) -> Value {
        revert_turn_input_schema()
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

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let offset = optional_u64(&input, "turn_offset", DEFAULT_REVERT_TURN_OFFSET);
        if offset == 0 || offset > MAX_REVERT_TURN_OFFSET {
            return Err(ToolError::invalid_input(format!(
                "turn_offset must be between 1 and {MAX_REVERT_TURN_OFFSET}; got {offset}",
            )));
        }

        let workspace = context.workspace.clone();
        let label = format!("revert_turn(offset={offset})");
        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            let repo = SnapshotRepo::open_or_init(&workspace)
                .map_err(|e| format!("Snapshot repo init failed: {e}"))?;
            let target = revert_pre_turn_offset(&repo, offset).map_err(|e| e.to_string())?;
            Ok(format!(
                "{label}: restored '{}' ({}). Workspace files reverted; conversation unchanged.",
                target.label,
                short_sha(target.id.as_str()),
            ))
        })
        .await
        .map_err(|e| ToolError::execution_failed(format!("revert_turn join failed: {e}")))?;

        match result {
            Ok(msg) => Ok(ToolResult::success(msg)),
            Err(e) => Ok(ToolResult::error(e)),
        }
    }
}

fn short_sha(sha: &str) -> &str {
    &sha[..sha.len().min(8)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::lock_test_env;
    use serde_json::json;
    use std::sync::MutexGuard;
    use tempfile::tempdir;

    struct HomeGuard {
        prev: Option<std::ffi::OsString>,
        _lock: MutexGuard<'static, ()>,
    }
    impl Drop for HomeGuard {
        fn drop(&mut self) {
            unsafe {
                match self.prev.take() {
                    Some(v) => std::env::set_var("HOME", v),
                    None => std::env::remove_var("HOME"),
                }
            }
        }
    }
    fn scoped_home(home: &std::path::Path) -> HomeGuard {
        let lock = lock_test_env();
        let prev = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", home);
        }
        HomeGuard { prev, _lock: lock }
    }

    #[tokio::test]
    async fn revert_turn_default_offset_restores_pre_turn_one() {
        let tmp = tempdir().unwrap();
        let workspace = tmp.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        let _guard = scoped_home(tmp.path());

        let repo = SnapshotRepo::open_or_init(&workspace).unwrap();
        std::fs::write(workspace.join("a.txt"), b"original").unwrap();
        repo.snapshot("pre-turn:1").unwrap();
        std::fs::write(workspace.join("a.txt"), b"modified").unwrap();
        repo.snapshot("post-turn:1").unwrap();

        let tool = RevertTurnTool;
        let ctx = ToolContext::new(workspace.clone());
        let r = tool.execute(json!({}), &ctx).await.expect("execute");
        assert!(r.success, "expected success: {r:?}");

        let content = std::fs::read_to_string(workspace.join("a.txt")).unwrap();
        assert_eq!(content, "original");
    }

    #[tokio::test]
    async fn revert_turn_invalid_offset_rejected() {
        let tmp = tempdir().unwrap();
        let workspace = tmp.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        let _guard = scoped_home(tmp.path());

        let tool = RevertTurnTool;
        let ctx = ToolContext::new(workspace);
        let r = tool.execute(json!({"turn_offset": 0}), &ctx).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn revert_turn_no_snapshots_returns_error_result() {
        let tmp = tempdir().unwrap();
        let workspace = tmp.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        let _guard = scoped_home(tmp.path());

        let tool = RevertTurnTool;
        let ctx = ToolContext::new(workspace);
        let r = tool.execute(json!({}), &ctx).await.expect("execute");
        assert!(!r.success);
        assert!(r.content.contains("out of range"));
    }
}
