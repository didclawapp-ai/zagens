//! Objective progress signal — git working-tree change detection (§4.8).
//!
//! "Let the facts speak": rather than asserting progress from a command
//! allowlist, observe whether the workspace actually changed. Language-agnostic
//! (covers `make` / custom scripts that the verification regex misses).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::runtime_api::workspace::run_git;

/// Stable signature of the workspace git working tree (`git status --porcelain`).
///
/// Returns `None` when the path is not a git repo or git is unavailable, so the
/// caller can degrade to the Phase 1 tool signals. Files ignored by `.gitignore`
/// (build artifacts, …) do not appear in porcelain output — a pure rebuild that
/// only touches ignored paths is correctly treated as "no source progress".
#[must_use]
pub fn workspace_change_signature(workspace: &Path) -> Option<String> {
    let inside = run_git(workspace, &["rev-parse", "--is-inside-work-tree"])?;
    if inside.trim() != "true" {
        return None;
    }
    let porcelain = run_git(workspace, &["status", "--porcelain=v1"])?;
    let mut hasher = DefaultHasher::new();
    porcelain.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> bool {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn git_available() -> bool {
        Command::new("git").arg("--version").output().is_ok()
    }

    #[test]
    fn signature_changes_when_worktree_changes() {
        if !git_available() {
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "lht-progress-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&dir);

        assert!(git(&dir, &["init"]), "git init");
        // Clean repo → a stable signature.
        let clean = workspace_change_signature(&dir);
        assert!(clean.is_some(), "git repo should yield a signature");

        // Introduce an untracked file → signature must change.
        std::fs::write(dir.join("a.txt"), b"hello").expect("write file");
        let dirty = workspace_change_signature(&dir);
        assert!(dirty.is_some());
        assert_ne!(clean, dirty, "worktree change must shift the signature");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
