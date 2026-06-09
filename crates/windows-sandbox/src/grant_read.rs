//! Elevated grant-read ACLs (PR-2.4, design §4.2).
//!
//! The elevated read-isolation model is **grant-exclusion**: the sandbox users
//! never receive a read grant on sensitive profile subdirectories
//! ([`USERPROFILE_ROOT_EXCLUSIONS`]) — what is not granted cannot be read by a
//! fresh local account. Non-excluded top-level profile entries get an
//! inheritable read+execute ACE for the sandbox users group; the profile root
//! itself gets a non-inheritable ACE so directory listing works without
//! leaking an inheritable allow into excluded subtrees.
//!
//! Applied grants are tracked in `.sandbox/system_read_grants.json` so
//! teardown (design §8.5 step 3) can revoke exactly what setup added.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::acl::{apply_grant_read_ace, apply_grant_read_ace_no_inherit, revoke_ace};
use crate::deny_read::USERPROFILE_SENSITIVE_DIRS;
use crate::paths::sandbox_dir;
use crate::token::LocalSid;

/// Workspace/agent metadata directories excluded from profile read grants in
/// addition to the credential directories shared with the unelevated deny list.
const AGENT_META_EXCLUSIONS: &[&str] = &[".zagens", ".agents", ".deepseek"];

/// Default per-run budget for applying profile read grants. Inheritable ACE
/// propagation into large profile subtrees is the expensive part, hence the
/// "async best-effort" framing in the design; within the elevated helper we
/// time-box instead so setup cannot hang indefinitely.
pub const PROFILE_GRANT_TIME_BUDGET: Duration = Duration::from_secs(120);

/// Returns true when `name` must never receive a read grant (grant-exclusion).
pub fn is_userprofile_root_exclusion(name: &str) -> bool {
    USERPROFILE_SENSITIVE_DIRS
        .iter()
        .chain(AGENT_META_EXCLUSIONS)
        .any(|excluded| name.eq_ignore_ascii_case(excluded))
}

/// All grant-exclusion names (sensitive credential dirs + agent metadata).
pub fn userprofile_root_exclusions() -> Vec<&'static str> {
    USERPROFILE_SENSITIVE_DIRS
        .iter()
        .chain(AGENT_META_EXCLUSIONS)
        .copied()
        .collect()
}

/// Plans the top-level profile entries that should receive an inheritable
/// read grant (everything except [`userprofile_root_exclusions`]).
pub fn plan_profile_read_grant_paths(profile: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(profile) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if is_userprofile_root_exclusion(name) {
            continue;
        }
        out.push(entry.path());
    }
    out.sort();
    out
}

#[derive(Debug, Default)]
pub struct GrantReadReport {
    pub granted: usize,
    pub failed: usize,
    /// True when the time budget expired before all grants were applied.
    pub truncated: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct TrackedReadGrants {
    paths: Vec<PathBuf>,
}

fn read_grants_state_path(home: &Path) -> PathBuf {
    sandbox_dir(home).join("system_read_grants.json")
}

fn load_tracked_read_grants(home: &Path) -> Vec<PathBuf> {
    let path = read_grants_state_path(home);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|txt| serde_json::from_str::<TrackedReadGrants>(&txt).ok())
        .map(|state| state.paths)
        .unwrap_or_default()
}

fn persist_tracked_read_grants(home: &Path, paths: &[PathBuf]) -> Result<()> {
    std::fs::create_dir_all(sandbox_dir(home))?;
    let state = TrackedReadGrants {
        paths: paths.to_vec(),
    };
    std::fs::write(
        read_grants_state_path(home),
        serde_json::to_string_pretty(&state)?,
    )?;
    Ok(())
}

/// Applies profile read grants for the sandbox users group, persisting the
/// granted paths for teardown. Idempotent: re-running re-applies the same ACEs.
pub fn apply_profile_read_grants<F>(
    home: &Path,
    profile: &Path,
    group_sid: &LocalSid,
    budget: Duration,
    mut log: F,
) -> Result<GrantReadReport>
where
    F: FnMut(&str),
{
    let deadline = Instant::now() + budget;
    let mut report = GrantReadReport::default();
    let mut granted_paths: Vec<PathBuf> = load_tracked_read_grants(home);

    // Non-inheritable grant on the profile root: list/traverse only.
    match apply_grant_read_ace_no_inherit(profile, group_sid.as_ptr()) {
        Ok(_) => {
            if !granted_paths.iter().any(|p| p == profile) {
                granted_paths.push(profile.to_path_buf());
            }
        }
        Err(err) => {
            report.failed += 1;
            log(&format!(
                "grant-read: profile root {} failed: {err}",
                profile.display()
            ));
        }
    }

    for path in plan_profile_read_grant_paths(profile) {
        if Instant::now() >= deadline {
            report.truncated = true;
            log("grant-read: time budget exhausted; remaining grants skipped");
            break;
        }
        match apply_grant_read_ace(&path, group_sid.as_ptr()) {
            Ok(_) => {
                report.granted += 1;
                if !granted_paths.iter().any(|p| p == &path) {
                    granted_paths.push(path);
                }
            }
            Err(err) => {
                report.failed += 1;
                log(&format!("grant-read: {} failed: {err}", path.display()));
            }
        }
    }

    persist_tracked_read_grants(home, &granted_paths)?;
    Ok(report)
}

/// Revokes every tracked read grant for the sandbox users group and removes
/// the state file. Returns the number of paths revoked.
pub fn revoke_read_grants(home: &Path, group_sid: &LocalSid) -> usize {
    let paths = load_tracked_read_grants(home);
    for path in &paths {
        revoke_ace(path, group_sid.as_ptr());
    }
    let _ = std::fs::remove_file(read_grants_state_path(home));
    paths.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusions_cover_credentials_and_agent_meta() {
        for name in [".ssh", ".aws", ".gnupg", ".zagens", ".agents", ".deepseek"] {
            assert!(
                is_userprofile_root_exclusion(name),
                "{name} must be excluded"
            );
        }
        assert!(is_userprofile_root_exclusion(".SSH"), "case-insensitive");
        assert!(!is_userprofile_root_exclusion("Documents"));
        assert!(!is_userprofile_root_exclusion("source"));
    }

    #[test]
    fn plan_skips_excluded_profile_children() {
        let dir =
            std::env::temp_dir().join(format!("zagens-grant-read-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ssh")).expect("mk .ssh");
        std::fs::create_dir_all(dir.join("Documents")).expect("mk Documents");
        std::fs::create_dir_all(dir.join(".zagens")).expect("mk .zagens");

        let planned = plan_profile_read_grant_paths(&dir);
        assert!(planned.contains(&dir.join("Documents")));
        assert!(!planned.iter().any(|p| p.ends_with(".ssh")));
        assert!(!planned.iter().any(|p| p.ends_with(".zagens")));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
