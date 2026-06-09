//! Workspace grant-write and protected-subdir deny-write ACEs.

use std::path::PathBuf;

use anyhow::Result;

use crate::acl::{apply_deny_write_ace, apply_grant_write_ace};
use crate::token::LocalSid;

pub fn apply_workspace_acls(
    writable_roots: &[PathBuf],
    protected_write_paths: &[PathBuf],
    workspace_cap_sid: &LocalSid,
) -> Result<()> {
    for root in writable_roots {
        if root.exists() {
            apply_grant_write_ace(root, workspace_cap_sid.as_ptr())?;
        }
    }
    for path in protected_write_paths {
        if path.exists() {
            apply_deny_write_ace(path, workspace_cap_sid.as_ptr())?;
        }
    }
    Ok(())
}

pub fn revoke_workspace_acls(
    writable_roots: &[PathBuf],
    protected_write_paths: &[PathBuf],
    workspace_cap_sid: &LocalSid,
) {
    for path in writable_roots.iter().chain(protected_write_paths) {
        crate::acl::revoke_ace(path, workspace_cap_sid.as_ptr());
    }
}
