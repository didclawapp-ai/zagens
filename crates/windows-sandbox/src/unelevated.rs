//! Unelevated sandbox session: lazy ACL setup + restricted-token spawn.

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use once_cell::sync::Lazy;

use crate::cap::load_or_create_cap_sids;
use crate::deny_read::{apply_deny_read_acls, plan_deny_read_acl_paths};
use crate::paths::zagens_home_from_env;
use crate::plan::WindowsExecPlan;
use crate::process::{CapturedOutput, ManagedProcess, SpawnStdio, spawn_with_stdio};
use crate::token::{LocalSid, create_restricted_token_with_capabilities};
use crate::workspace_acl::apply_workspace_acls;

static UNELEVATED_SESSION: Lazy<Mutex<Option<UnelevatedSession>>> = Lazy::new(|| Mutex::new(None));

struct UnelevatedSession {
    cap_sid: String,
    deny_read_paths: Vec<PathBuf>,
}

pub fn ensure_unelevated_acls(plan: &WindowsExecPlan) -> Result<()> {
    let home = zagens_home_from_env();
    let caps = load_or_create_cap_sids(&home)?;
    let cap_sid = LocalSid::from_string(&caps.workspace)?;

    let user_profile = std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .context("USERPROFILE not set")?;

    let mut session = UNELEVATED_SESSION
        .lock()
        .map_err(|_| anyhow::anyhow!("unelevated session lock poisoned"))?;

    let needs_workspace_setup = session
        .as_ref()
        .is_none_or(|existing| existing.cap_sid != caps.workspace);

    if needs_workspace_setup {
        apply_workspace_acls(&plan.writable_roots, &plan.protected_write_paths, &cap_sid)?;

        crate::teardown::persist_tracked_workspace_paths(
            &home,
            &plan.writable_roots,
            &plan.protected_write_paths,
        )?;

        *session = Some(UnelevatedSession {
            cap_sid: caps.workspace.clone(),
            deny_read_paths: Vec::new(),
        });
    }

    // Deny-read must run on every enforced spawn: directory ACEs do not
    // retroactively inherit to files created before the ACE (e.g. `.ssh/id_rsa`).
    if plan.apply_deny_read {
        let deny_read_paths = plan_deny_read_acl_paths(&user_profile, &plan.writable_roots);
        apply_deny_read_acls(&deny_read_paths, &cap_sid)?;
        if !deny_read_paths.is_empty() {
            crate::teardown::persist_tracked_deny_read_paths(&home, &deny_read_paths)?;
        }
        if let Some(existing) = session.as_mut() {
            existing.deny_read_paths = deny_read_paths;
        }
    }

    Ok(())
}

pub fn spawn(plan: &WindowsExecPlan, stdio: SpawnStdio) -> Result<ManagedProcess> {
    ensure_unelevated_acls(plan)?;

    let home = zagens_home_from_env();
    let caps = load_or_create_cap_sids(&home)?;
    let token = create_restricted_token_with_capabilities(&[&caps.workspace])?;

    spawn_with_stdio(token.handle(), &plan.argv, &plan.cwd, &plan.env, stdio)
}

pub fn spawn_sync(
    plan: &WindowsExecPlan,
    stdin_data: Option<&str>,
    timeout: Option<std::time::Duration>,
) -> Result<CapturedOutput> {
    let process = spawn(
        plan,
        SpawnStdio {
            capture_stdout: true,
            capture_stderr: true,
            stdin_data: stdin_data.map(str::to_string),
        },
    )?;
    let mut process = process;
    process.wait(timeout)
}

pub fn session_deny_read_paths() -> Vec<PathBuf> {
    UNELEVATED_SESSION
        .lock()
        .ok()
        .and_then(|session| session.as_ref().map(|s| s.deny_read_paths.clone()))
        .unwrap_or_default()
}
