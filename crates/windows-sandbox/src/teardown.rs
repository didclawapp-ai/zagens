//! Teardown unelevated sandbox ACL state (no WFP / sandbox users in Phase 1).

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::cap::load_or_create_cap_sids;
use crate::deny_read::revoke_deny_read_acls;
use crate::paths::{cap_sid_file, sandbox_dir, zagens_home_from_env};
use crate::token::LocalSid;
use crate::unelevated::session_deny_read_paths;
use crate::workspace_acl::revoke_workspace_acls;

pub struct TeardownReport {
    pub revoked_paths: usize,
    pub cap_sid_removed: bool,
}

pub fn teardown_unelevated(keep_logs: bool) -> Result<TeardownReport> {
    let home = zagens_home_from_env();
    let cap_path = cap_sid_file(&home);
    let caps = if cap_path.exists() {
        Some(load_or_create_cap_sids(&home)?)
    } else {
        None
    };

    let mut revoked_paths = 0usize;
    if let Some(caps) = &caps {
        let cap_sid = LocalSid::from_string(&caps.workspace)?;
        let mut deny_paths = session_deny_read_paths();
        if deny_paths.is_empty() {
            deny_paths = load_tracked_deny_read_paths(&home);
        }
        revoke_deny_read_acls(&deny_paths, &cap_sid);
        revoked_paths += deny_paths.len();
        clear_tracked_deny_read_paths(&home);

        let tracked = load_tracked_workspace_paths(&home);
        revoke_workspace_acls(&tracked.roots, &tracked.protected, &cap_sid);
        revoked_paths += tracked.roots.len() + tracked.protected.len();
    }

    if !keep_logs && cap_path.exists() {
        let _ = std::fs::remove_file(&cap_path);
    }

    Ok(TeardownReport {
        revoked_paths,
        cap_sid_removed: !keep_logs && !cap_path.exists(),
    })
}

#[derive(Serialize, Deserialize)]
struct TrackedWorkspacePaths {
    roots: Vec<PathBuf>,
    protected: Vec<PathBuf>,
}

impl TrackedWorkspacePaths {
    fn from_json(txt: &str) -> Option<Self> {
        serde_json::from_str(txt).ok()
    }
}

fn load_tracked_workspace_paths(home: &std::path::Path) -> TrackedWorkspacePaths {
    let path = sandbox_dir(home).join("unelevated_workspace_paths.json");
    if let Ok(txt) = std::fs::read_to_string(&path)
        && let Some(parsed) = TrackedWorkspacePaths::from_json(&txt)
    {
        return parsed;
    }
    TrackedWorkspacePaths {
        roots: Vec::new(),
        protected: Vec::new(),
    }
}

#[derive(Serialize, Deserialize)]
struct TrackedDenyReadPaths {
    paths: Vec<PathBuf>,
}

pub fn persist_tracked_deny_read_paths(home: &std::path::Path, paths: &[PathBuf]) -> Result<()> {
    std::fs::create_dir_all(sandbox_dir(home))?;
    let payload = TrackedDenyReadPaths {
        paths: paths.to_vec(),
    };
    let path = sandbox_dir(home).join("unelevated_deny_read_paths.json");
    std::fs::write(path, serde_json::to_string_pretty(&payload)?)?;
    Ok(())
}

fn load_tracked_deny_read_paths(home: &std::path::Path) -> Vec<PathBuf> {
    let path = sandbox_dir(home).join("unelevated_deny_read_paths.json");
    if let Ok(txt) = std::fs::read_to_string(&path)
        && let Ok(parsed) = serde_json::from_str::<TrackedDenyReadPaths>(&txt)
    {
        return parsed.paths;
    }
    Vec::new()
}

fn clear_tracked_deny_read_paths(home: &std::path::Path) {
    let path = sandbox_dir(home).join("unelevated_deny_read_paths.json");
    let _ = std::fs::remove_file(path);
}

pub fn persist_tracked_workspace_paths(
    home: &std::path::Path,
    roots: &[PathBuf],
    protected: &[PathBuf],
) -> Result<()> {
    std::fs::create_dir_all(sandbox_dir(home))?;
    let payload = TrackedWorkspacePaths {
        roots: roots.to_vec(),
        protected: protected.to_vec(),
    };
    let path = sandbox_dir(home).join("unelevated_workspace_paths.json");
    std::fs::write(path, serde_json::to_string_pretty(&payload)?)?;
    Ok(())
}
