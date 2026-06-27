//! Thread worktree allocation and cleanup (P1).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tracing::warn;
use uuid::Uuid;
use zagens_runtime_adapters::worktree::{
    WorktreesRuntimeConfig, create_craft_worktree, create_session_worktree, is_git_repository,
    remove_worktree, resolve_git_root,
};
use zagens_runtime_orchestrator::runtime_threads::ThreadRecord;

/// Resolved workspace + optional worktree metadata for a new thread.
pub struct ThreadWorkspaceBinding {
    pub workspace: PathBuf,
    pub git_root: Option<PathBuf>,
    pub worktree_name: Option<String>,
}

pub fn resolve_thread_workspace_binding(
    requested_workspace: PathBuf,
    thread_id: &str,
    use_worktree: Option<bool>,
    config: &WorktreesRuntimeConfig,
) -> Result<ThreadWorkspaceBinding> {
    if !config.use_worktree_for_request(use_worktree) {
        return Ok(ThreadWorkspaceBinding {
            workspace: requested_workspace,
            git_root: None,
            worktree_name: None,
        });
    }

    if !is_git_repository(&requested_workspace) {
        bail!(
            "use_worktree requires a git repository workspace ({} is not inside a git repo)",
            requested_workspace.display()
        );
    }

    let git_root = resolve_git_root(&requested_workspace)?;
    let name = format!("session-{thread_id}");
    let wt = create_session_worktree(&git_root, config, &name)
        .with_context(|| format!("failed to create worktree for thread {thread_id}"))?;

    Ok(ThreadWorkspaceBinding {
        workspace: wt.worktree_path,
        git_root: Some(wt.git_root),
        worktree_name: Some(wt.worktree_name),
    })
}

pub fn maybe_prune_thread_worktree(thread: &ThreadRecord, config: &WorktreesRuntimeConfig) {
    if !config.enabled || !config.prune_on_thread_archive {
        return;
    }
    let (Some(git_root), Some(_name)) = (thread.git_root.as_ref(), thread.worktree_name.as_ref())
    else {
        return;
    };
    let worktree_path = thread.workspace.clone();
    if let Err(err) = remove_worktree(git_root, &worktree_path, true) {
        warn!(
            thread_id = %thread.id,
            path = %worktree_path.display(),
            "worktree prune on archive failed: {err}"
        );
    }
}

/// Short suffix for CRAFT parallel worktree names.
#[must_use]
pub fn craft_worktree_suffix() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

#[must_use]
pub fn parent_git_root_for_spawn(parent_workspace: &Path) -> Option<PathBuf> {
    resolve_git_root(parent_workspace).ok()
}

/// CRAFT parallel auto-worktree when `task_id` is set and config allows it.
pub fn maybe_allocate_craft_worktree(
    parent_workspace: &Path,
    config: &WorktreesRuntimeConfig,
    task_id: &str,
) -> Option<PathBuf> {
    if !config.enabled || !config.auto_on_craft_parallel {
        return None;
    }
    match create_craft_worktree(parent_workspace, config, task_id, &craft_worktree_suffix()) {
        Ok(wt) => Some(wt.worktree_path),
        Err(err) => {
            warn!(
                task_id,
                "craft parallel worktree auto-allocation skipped: {err:#}"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_skips_when_disabled() {
        let cfg = WorktreesRuntimeConfig {
            enabled: false,
            ..WorktreesRuntimeConfig::default()
        };
        let binding = resolve_thread_workspace_binding(
            PathBuf::from("/tmp/ws"),
            "thr_test",
            Some(true),
            &cfg,
        )
        .expect("binding");
        assert_eq!(binding.workspace, PathBuf::from("/tmp/ws"));
        assert!(binding.git_root.is_none());
    }

    fn init_git_repo(path: &Path) {
        use std::process::Command;
        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(path)
            .output()
            .expect("git init");
        std::fs::write(path.join("README.md"), "test\n").unwrap();
        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(path)
            .output()
            .expect("git add");
        Command::new("git")
            .args(["commit", "-m", "init"])
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@test")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@test")
            .current_dir(path)
            .output()
            .expect("git commit");
    }

    #[test]
    fn create_binding_and_archive_prune_removes_worktree() {
        use chrono::Utc;
        use std::process::Command;
        use zagens_core::coherence::CoherenceState;
        use zagens_runtime_orchestrator::runtime_threads::{
            CURRENT_RUNTIME_SCHEMA_VERSION, default_thread_task_type,
        };

        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_git_repo(&repo);

        let cfg = WorktreesRuntimeConfig {
            enabled: true,
            prune_on_thread_archive: true,
            ..WorktreesRuntimeConfig::default()
        };
        let binding =
            resolve_thread_workspace_binding(repo.clone(), "thr_prune_test", Some(true), &cfg)
                .expect("binding");
        assert!(binding.worktree_name.is_some());
        assert!(binding.workspace.exists());
        assert_ne!(binding.workspace, repo);

        let now = Utc::now();
        let thread = ThreadRecord {
            schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
            id: "thr_prune_test".to_string(),
            created_at: now,
            updated_at: now,
            model: "deepseek-chat".to_string(),
            workspace: binding.workspace.clone(),
            mode: "agent".to_string(),
            allow_shell: false,
            trust_mode: false,
            auto_approve: false,
            latest_turn_id: None,
            latest_response_bookmark: None,
            archived: false,
            system_prompt: None,
            task_id: None,
            title: None,
            task_type: default_thread_task_type(),
            coherence_state: CoherenceState::default(),
            scratchpad_run_id: None,
            scratchpad_run_history: None,
            checklist_snapshot: None,
            plan_snapshot: None,
            config_overlay: None,
            git_root: binding.git_root.clone(),
            worktree_name: binding.worktree_name.clone(),
        };
        maybe_prune_thread_worktree(&thread, &cfg);
        assert!(!binding.workspace.exists());
    }
}
