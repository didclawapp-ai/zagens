//! Git worktree lifecycle for isolated agent sessions (P1).
//!
//! Managed worktrees live under `<git-root>/.worktrees/<name>/` by default,
//! matching `.gitignore` and snapshot [`crate::snapshot::paths`] hashing.

use std::path::{Path, PathBuf};
use std::process::Output;

use anyhow::{Context, Result, bail};
use zagens_config::WorktreesToml;

/// Resolved worktree settings with defaults applied.
#[derive(Debug, Clone)]
pub struct WorktreesRuntimeConfig {
    pub enabled: bool,
    pub root_dir: PathBuf,
    pub auto_on_new_session: bool,
    pub auto_on_craft_parallel: bool,
    pub prune_on_thread_archive: bool,
    pub max_worktrees_per_repo: usize,
    pub default_branch: Option<String>,
}

impl Default for WorktreesRuntimeConfig {
    fn default() -> Self {
        Self::from_toml(&WorktreesToml::default())
    }
}

impl WorktreesRuntimeConfig {
    #[must_use]
    pub fn from_toml(toml: &WorktreesToml) -> Self {
        Self {
            enabled: toml.enabled,
            root_dir: PathBuf::from(toml.root_dir.trim()),
            auto_on_new_session: toml.auto_on_new_session,
            auto_on_craft_parallel: toml.auto_on_craft_parallel,
            prune_on_thread_archive: toml.prune_on_thread_archive,
            max_worktrees_per_repo: toml.max_worktrees_per_repo.max(1),
            default_branch: toml
                .default_branch
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        }
    }

    /// Whether this thread/session should allocate a worktree.
    #[must_use]
    pub fn use_worktree_for_request(&self, requested: Option<bool>) -> bool {
        if !self.enabled {
            return false;
        }
        match requested {
            Some(true) => true,
            Some(false) => false,
            None => self.auto_on_new_session,
        }
    }
}

/// A newly created session-scoped worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWorktree {
    pub git_root: PathBuf,
    pub worktree_path: PathBuf,
    pub worktree_name: String,
    pub branch_name: String,
}

/// Resolve the git repository root containing `path`.
pub fn resolve_git_root(path: &Path) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("workspace path not found: {}", path.display()))?;
    let out = run_git(&canonical, &["rev-parse", "--show-toplevel"])?;
    if !out.status.success() {
        bail!(
            "not a git repository: {}",
            stderr_trim(&out).unwrap_or_else(|| "git rev-parse failed".into())
        );
    }
    let root = PathBuf::from(stdout_trim(&out)?);
    Ok(root)
}

#[must_use]
pub fn is_git_repository(path: &Path) -> bool {
    resolve_git_root(path).is_ok()
}

/// Create an isolated worktree for a new runtime thread.
pub fn create_session_worktree(
    git_root: &Path,
    config: &WorktreesRuntimeConfig,
    name: &str,
) -> Result<SessionWorktree> {
    let name = sanitize_worktree_name(name)?;
    let worktree_path = git_root.join(&config.root_dir).join(&name);
    if worktree_path.exists() {
        bail!("worktree path already exists: {}", worktree_path.display());
    }

    let managed = list_managed_worktrees(git_root, &config.root_dir)?;
    if managed.len() >= config.max_worktrees_per_repo {
        bail!(
            "worktree limit reached (max {} for {})",
            config.max_worktrees_per_repo,
            git_root.display()
        );
    }

    std::fs::create_dir_all(git_root.join(&config.root_dir))
        .with_context(|| format!("create {}", git_root.join(&config.root_dir).display()))?;

    let branch_name = format!("zagens/{name}");
    let base = resolve_worktree_base_ref(git_root, config.default_branch.as_deref())?;

    let mut args = vec![
        "worktree".to_string(),
        "add".to_string(),
        "-b".to_string(),
        branch_name.clone(),
        worktree_path.display().to_string(),
        base,
    ];
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_git(git_root, &arg_refs)?;
    if !out.status.success() {
        // Branch may already exist from a prior failed attempt — retry without -b.
        args = vec![
            "worktree".to_string(),
            "add".to_string(),
            worktree_path.display().to_string(),
            branch_name.clone(),
        ];
        let retry_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let retry = run_git(git_root, &retry_refs)?;
        if !retry.status.success() {
            bail!(
                "git worktree add failed: {}",
                stderr_trim(&retry).unwrap_or_else(|| stderr_trim(&out).unwrap_or_default())
            );
        }
    }

    Ok(SessionWorktree {
        git_root: git_root.to_path_buf(),
        worktree_path,
        worktree_name: name,
        branch_name,
    })
}

/// Create a CRAFT / sub-agent parallel worktree under the parent workspace git root.
pub fn create_craft_worktree(
    parent_workspace: &Path,
    config: &WorktreesRuntimeConfig,
    task_id: &str,
    suffix: &str,
) -> Result<SessionWorktree> {
    if !config.enabled || !config.auto_on_craft_parallel {
        bail!("craft worktree auto-allocation is disabled");
    }
    let git_root = resolve_git_root(parent_workspace)?;
    let name = sanitize_worktree_name(&format!("craft-{task_id}-{suffix}"))?;
    create_session_worktree(&git_root, config, &name)
}

/// Remove a managed worktree directory and prune stale registration.
pub fn remove_worktree(git_root: &Path, worktree_path: &Path, force: bool) -> Result<()> {
    if !worktree_path.exists() {
        return Ok(());
    }
    let mut args = vec!["worktree".to_string(), "remove".to_string()];
    if force {
        args.push("--force".to_string());
    }
    let path_arg = worktree_path.to_string_lossy().into_owned();
    args.push(path_arg);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_git(git_root, &arg_refs)?;
    if !out.status.success() {
        bail!(
            "git worktree remove failed: {}",
            stderr_trim(&out).unwrap_or_else(|| "unknown error".into())
        );
    }
    let _ = run_git(git_root, &["worktree", "prune"]);
    Ok(())
}

/// List worktree paths under `root_dir` relative to `git_root`.
pub fn list_managed_worktrees(git_root: &Path, root_dir: &Path) -> Result<Vec<PathBuf>> {
    let base = git_root.join(root_dir);
    if !base.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&base).with_context(|| format!("read {}", base.display()))? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

fn resolve_worktree_base_ref(git_root: &Path, configured: Option<&str>) -> Result<String> {
    if let Some(branch) = configured {
        return Ok(branch.to_string());
    }
    let head = run_git(git_root, &["symbolic-ref", "--short", "HEAD"])?;
    if head.status.success() {
        return Ok(stdout_trim(&head)?);
    }
    Ok("HEAD".to_string())
}

fn sanitize_worktree_name(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("worktree name must not be empty");
    }
    let safe: String = trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if safe.is_empty() {
        bail!("worktree name invalid after sanitization");
    }
    Ok(safe)
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<Output> {
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .output()
        .with_context(|| "failed to spawn git")?;
    Ok(output)
}

fn stdout_trim(out: &Output) -> Result<String> {
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn stderr_trim(out: &Output) -> Option<String> {
    let s = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::tempdir;

    fn init_git_repo(path: &Path) {
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
    fn session_worktree_isolated_under_dot_worktrees() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_git_repo(&repo);

        let cfg = WorktreesRuntimeConfig::default();
        let wt = create_session_worktree(&repo, &cfg, "session-thr_test01").expect("create");
        assert!(wt.worktree_path.starts_with(repo.join(".worktrees")));
        assert!(wt.worktree_path.join("README.md").exists());
        assert_ne!(wt.worktree_path, repo);

        remove_worktree(&repo, &wt.worktree_path, true).expect("remove");
    }

    #[test]
    fn sanitize_rejects_empty() {
        assert!(sanitize_worktree_name("").is_err());
        assert_eq!(sanitize_worktree_name("thr/abc").unwrap(), "thr-abc");
    }
}
