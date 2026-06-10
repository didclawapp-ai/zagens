//! Deny-read ACL planning and Gate G0 gating.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::acl::{
    apply_deny_read_ace, has_inherited_deny_read_ace, restore_inherited_dacl, revoke_ace,
};
use crate::paths::{poc_result_file, zagens_home_from_env};
use crate::token::LocalSid;

pub(crate) const USERPROFILE_SENSITIVE_DIRS: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".azure",
    ".kube",
    ".docker",
    ".config",
    ".npm",
    ".pki",
    ".terraform.d",
    ".tsh",
    ".brev",
];

const MAX_PROPAGATION_DEPTH: u32 = 8;
const MAX_PROPAGATION_FILES_PER_ROOT: usize = 5000;

pub fn unelevated_deny_read_enabled() -> bool {
    let path = poc_result_file(&zagens_home_from_env());
    let Ok(txt) = std::fs::read_to_string(&path) else {
        return false;
    };
    txt.contains("\"result\": \"pass\"")
        || txt.contains("\"result\":\"pass\"")
        || txt.contains("\"result\": \"pass\"")
}

pub fn plan_deny_read_acl_paths(user_profile: &Path, writable_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for name in USERPROFILE_SENSITIVE_DIRS {
        paths.push(user_profile.join(name));
    }
    for root in writable_roots {
        for name in [".zagens", ".agents", ".deepseek"] {
            paths.push(root.join(name));
        }
    }
    dedupe_paths(paths)
}

pub fn apply_deny_read_acls(paths: &[PathBuf], cap_sid: &LocalSid) -> Result<()> {
    for path in paths {
        let _ = apply_deny_read_ace(path, cap_sid.as_ptr())?;
        if path.is_dir() {
            propagate_deny_read_to_existing_children(path, cap_sid, 0, &mut 0)?;
        }
    }
    Ok(())
}

pub fn revoke_deny_read_acls(paths: &[PathBuf], cap_sid: &LocalSid) {
    for path in paths {
        if path.is_dir() {
            revoke_deny_read_from_existing_children(path, 0, &mut 0);
        }
        revoke_ace(path, cap_sid.as_ptr());
    }
}

fn propagate_deny_read_to_existing_children(
    dir: &Path,
    cap_sid: &LocalSid,
    depth: u32,
    files_touched: &mut usize,
) -> Result<()> {
    if depth >= MAX_PROPAGATION_DEPTH || *files_touched >= MAX_PROPAGATION_FILES_PER_ROOT {
        return Ok(());
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        if *files_touched >= MAX_PROPAGATION_FILES_PER_ROOT {
            break;
        }
        let path = entry.path();
        if path.is_file() {
            if !has_inherited_deny_read_ace(&path, cap_sid.as_ptr()) {
                let _ = restore_inherited_dacl(&path)?;
                *files_touched += 1;
            }
        } else if path.is_dir() {
            propagate_deny_read_to_existing_children(&path, cap_sid, depth + 1, files_touched)?;
        }
    }
    Ok(())
}

fn revoke_deny_read_from_existing_children(dir: &Path, depth: u32, files_touched: &mut usize) {
    if depth >= MAX_PROPAGATION_DEPTH || *files_touched >= MAX_PROPAGATION_FILES_PER_ROOT {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if *files_touched >= MAX_PROPAGATION_FILES_PER_ROOT {
            break;
        }
        let path = entry.path();
        if path.is_file() {
            let _ = restore_inherited_dacl(&path);
            *files_touched += 1;
        } else if path.is_dir() {
            revoke_deny_read_from_existing_children(&path, depth + 1, files_touched);
        }
    }
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        if !out.iter().any(|existing| existing == &path) {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_includes_ssh_and_workspace_meta() {
        let profile = PathBuf::from(r"C:\Users\alice");
        let roots = vec![PathBuf::from(r"D:\proj")];
        let paths = plan_deny_read_acl_paths(&profile, &roots);
        assert!(paths.contains(&profile.join(".ssh")));
        assert!(paths.contains(&PathBuf::from(r"D:\proj\.zagens")));
    }

    // Read-isolation invariant tests below are gated on G0 (`unelevated_deny_read_enabled`).
    //
    // Unelevated read isolation relies on a cap-SID deny-read ACE being honored by the
    // restricted token. With `WRITE_RESTRICTED`, the restricting/capability SIDs are only
    // evaluated for *write* access, so a deny-read ACE keyed to the synthetic cap SID can
    // never match on a read. G0 therefore reports `fail` on this machine and deny-read is
    // disabled at runtime. These tests only assert the read-block invariant when G0 claims
    // `pass` (e.g. a future elevated/sandbox-user path that genuinely enforces it); under a
    // failing G0 they skip rather than assert a guarantee the platform cannot provide.
    #[test]
    #[cfg(windows)]
    fn prepare_like_plan_fields_do_not_break_id_rsa_block() {
        if !super::unelevated_deny_read_enabled() {
            eprintln!("skip: G0 deny-read PoC not pass (unelevated read isolation unavailable)");
            return;
        }
        let profile = std::env::var("USERPROFILE")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Users\Administrator"));
        let id_rsa = profile.join(".ssh").join("id_rsa");
        if !id_rsa.is_file() {
            return;
        }
        let workspace = PathBuf::from(r"F:\DeepSeek-TUI-desktop");
        if !workspace.is_dir() {
            return;
        }
        let canonical = workspace
            .canonicalize()
            .unwrap_or_else(|_| workspace.clone());
        let mut protected = Vec::new();
        for name in [".git", ".zagens", ".agents", ".deepseek"] {
            protected.push(canonical.join(name));
        }
        let command = format!("type {}", id_rsa.display());
        let plan = crate::plan::plan_exec(crate::plan::PlanInput {
            program: "powershell".into(),
            args: vec!["-Command".into(), command],
            cwd: workspace.clone(),
            env: std::collections::HashMap::new(),
            writable_roots: vec![canonical.clone(), workspace.clone()],
            protected_write_paths: protected,
            network_allowed: false,
            mode: crate::plan::WindowsSandboxMode::Unelevated,
            private_desktop: false,
            tty: false,
        })
        .expect("plan");
        let out = crate::spawn_sync(&plan, None, Some(std::time::Duration::from_secs(15)))
            .expect("spawn_sync");
        assert!(
            !out.stdout.contains("BEGIN OPENSSH PRIVATE KEY"),
            "prepare-like plan leaked (argv={:?})",
            plan.argv
        );
    }

    #[test]
    #[cfg(windows)]
    fn spawn_sync_alone_blocks_id_rsa_without_prior_apply() {
        if !super::unelevated_deny_read_enabled() {
            eprintln!("skip: G0 deny-read PoC not pass (unelevated read isolation unavailable)");
            return;
        }
        let profile = std::env::var("USERPROFILE")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Users\Administrator"));
        let id_rsa = profile.join(".ssh").join("id_rsa");
        if !id_rsa.is_file() {
            return;
        }

        let plan = crate::plan::plan_exec(crate::plan::PlanInput {
            program: "powershell".into(),
            args: vec!["-Command".into(), format!("type {}", id_rsa.display())],
            cwd: PathBuf::from(r"F:\DeepSeek-TUI-desktop"),
            env: std::collections::HashMap::new(),
            writable_roots: vec![PathBuf::from(r"F:\DeepSeek-TUI-desktop")],
            protected_write_paths: vec![],
            network_allowed: false,
            mode: crate::plan::WindowsSandboxMode::Unelevated,
            private_desktop: false,
            tty: false,
        })
        .expect("plan");
        assert!(
            plan.argv[2].contains('"'),
            "plan_exec must harden bare drive paths: {:?}",
            plan.argv
        );
        let out = crate::spawn_sync(&plan, None, Some(std::time::Duration::from_secs(15)))
            .expect("spawn_sync");
        assert!(
            !out.stdout.contains("BEGIN OPENSSH PRIVATE KEY"),
            "spawn_sync alone must block id_rsa (exit={})",
            out.exit_code
        );
    }

    #[test]
    #[cfg(windows)]
    fn propagate_inherited_deny_blocks_id_rsa_read() {
        use std::collections::HashMap;

        if !super::unelevated_deny_read_enabled() {
            eprintln!("skip: G0 deny-read PoC not pass (unelevated read isolation unavailable)");
            return;
        }
        let profile = std::env::var("USERPROFILE")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Users\Administrator"));
        let ssh_dir = profile.join(".ssh");
        let id_rsa = ssh_dir.join("id_rsa");
        if !id_rsa.is_file() {
            return;
        }

        let caps = crate::cap::load_or_create_cap_sids(&crate::paths::zagens_home_from_env())
            .expect("cap sid");
        let cap_sid = crate::token::LocalSid::from_string(&caps.workspace).expect("cap sid parse");

        apply_deny_read_acls(&[ssh_dir.clone()], &cap_sid).expect("apply deny read");

        assert!(
            has_inherited_deny_read_ace(&id_rsa, cap_sid.as_ptr()),
            "id_rsa should inherit cap SID deny-read ACE from .ssh"
        );

        let token = crate::token::create_restricted_token_with_capabilities(&[&caps.workspace])
            .expect("token");
        let argv = vec![
            "cmd".to_string(),
            "/C".to_string(),
            format!("type \"{}\"", id_rsa.display()),
        ];
        let out = crate::process::run_as_user(
            token.handle(),
            &argv,
            &PathBuf::from(r"F:\DeepSeek-TUI-desktop"),
            &HashMap::new(),
        )
        .expect("spawn probe");
        assert!(
            !out.stdout.contains("BEGIN OPENSSH PRIVATE KEY"),
            "restricted token must not read id_rsa (exit={})",
            out.exit_code
        );

        let plan = crate::plan::plan_exec(crate::plan::PlanInput {
            program: "powershell".into(),
            args: vec!["-Command".into(), format!("type \"{}\"", id_rsa.display())],
            cwd: PathBuf::from(r"F:\DeepSeek-TUI-desktop"),
            env: HashMap::new(),
            writable_roots: vec![PathBuf::from(r"F:\DeepSeek-TUI-desktop")],
            protected_write_paths: vec![],
            network_allowed: false,
            mode: crate::plan::WindowsSandboxMode::Unelevated,
            private_desktop: false,
            tty: false,
        })
        .expect("plan");
        let spawn_out = crate::spawn_sync(&plan, None, Some(std::time::Duration::from_secs(15)))
            .expect("spawn_sync");
        assert!(
            !spawn_out.stdout.contains("BEGIN OPENSSH PRIVATE KEY"),
            "spawn_sync must not read id_rsa (exit={})",
            spawn_out.exit_code
        );
    }
}
