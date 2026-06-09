//! Elevated deny-read backstop + persisted state (PR-2.5, design §4.4).
//!
//! Grant-exclusion (see `grant_read.rs`) is the elevated read-isolation
//! mainline; the explicit deny-read ACE here is the **backstop** for reparse
//! points and broad inherited grants. Each sensitive path is planned twice —
//! lexical and canonicalized (junction/symlink target) — so a reparse point
//! inside the profile cannot redirect the deny away from the real directory
//! (design §4.4.2 dual-path planning).
//!
//! Applied paths are tracked in `.sandbox/deny_read_state.json` so refresh can
//! sync incrementally and teardown can revoke exactly what setup added.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::acl::{apply_deny_read_ace, revoke_ace};
use crate::deny_read::USERPROFILE_SENSITIVE_DIRS;
use crate::paths::sandbox_dir;
use crate::token::LocalSid;

#[derive(Serialize, Deserialize, Default)]
struct DenyReadState {
    paths: Vec<PathBuf>,
}

fn deny_read_state_path(home: &Path) -> PathBuf {
    sandbox_dir(home).join("deny_read_state.json")
}

fn load_state(home: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(deny_read_state_path(home))
        .ok()
        .and_then(|txt| serde_json::from_str::<DenyReadState>(&txt).ok())
        .map(|state| state.paths)
        .unwrap_or_default()
}

fn persist_state(home: &Path, paths: &[PathBuf]) -> Result<()> {
    std::fs::create_dir_all(sandbox_dir(home))?;
    let state = DenyReadState {
        paths: paths.to_vec(),
    };
    std::fs::write(
        deny_read_state_path(home),
        serde_json::to_string_pretty(&state)?,
    )?;
    Ok(())
}

/// Plans elevated deny-read targets: every sensitive profile subdirectory,
/// lexical **and** canonical (resolving reparse points for existing paths).
pub fn plan_elevated_deny_read_paths(profile: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let push_unique = |path: PathBuf, out: &mut Vec<PathBuf>| {
        if !out.iter().any(|existing| existing == &path) {
            out.push(path);
        }
    };
    for name in USERPROFILE_SENSITIVE_DIRS {
        let lexical = profile.join(name);
        if let Ok(canonical) = lexical.canonicalize() {
            // `canonicalize` returns the reparse target with a `\\?\` prefix;
            // keep it distinct from the lexical path so both carry the deny.
            push_unique(canonical, &mut out);
        }
        push_unique(lexical, &mut out);
    }
    out
}

/// Applies the elevated deny-read backstop for the sandbox users group and
/// syncs `.sandbox/deny_read_state.json` (union of previous + current paths).
/// Returns the number of paths carrying the deny after the sync.
pub fn sync_elevated_deny_read<F>(
    home: &Path,
    profile: &Path,
    group_sid: &LocalSid,
    mut log: F,
) -> Result<usize>
where
    F: FnMut(&str),
{
    let mut tracked = load_state(home);
    let planned = plan_elevated_deny_read_paths(profile);
    for path in &planned {
        match apply_deny_read_ace(path, group_sid.as_ptr()) {
            Ok(_) => {
                if !tracked.iter().any(|existing| existing == path) {
                    tracked.push(path.clone());
                }
            }
            Err(err) => {
                log(&format!(
                    "deny-read backstop: {} failed: {err}",
                    path.display()
                ));
            }
        }
    }
    persist_state(home, &tracked)?;
    Ok(tracked.len())
}

/// Revokes every tracked elevated deny-read ACE and removes the state file.
/// Returns the number of paths revoked.
pub fn revoke_elevated_deny_read(home: &Path, group_sid: &LocalSid) -> usize {
    let paths = load_state(home);
    for path in &paths {
        revoke_ace(path, group_sid.as_ptr());
    }
    let _ = std::fs::remove_file(deny_read_state_path(home));
    paths.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_covers_sensitive_dirs_lexically() {
        let profile = PathBuf::from(r"C:\Users\nobody-zagens-test");
        let planned = plan_elevated_deny_read_paths(&profile);
        assert!(planned.contains(&profile.join(".ssh")));
        assert!(planned.contains(&profile.join(".aws")));
    }

    #[cfg(windows)]
    #[test]
    fn plan_adds_canonical_target_for_existing_paths() {
        let dir =
            std::env::temp_dir().join(format!("zagens-deny-state-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ssh")).expect("mk .ssh");

        let planned = plan_elevated_deny_read_paths(&dir);
        let lexical = dir.join(".ssh");
        let canonical = lexical.canonicalize().expect("canonicalize");
        assert!(planned.contains(&lexical));
        assert!(planned.contains(&canonical));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
