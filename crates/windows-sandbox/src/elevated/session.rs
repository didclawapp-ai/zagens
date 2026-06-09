//! Shared elevated session bootstrap: workspace ACLs + sandbox creds + runner
//! transport. Used by both the synchronous capture path and background spawn.

use std::fs::File;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::cap::load_or_create_cap_sids;
use crate::elevated::ipc::SpawnRequest;
use crate::elevated::runner_client::spawn_runner_transport;
use crate::identity_creds::require_sandbox_creds;
use crate::paths::{sandbox_dir, zagens_home_from_env};
use crate::plan::WindowsExecPlan;
use crate::token::LocalSid;
use crate::workspace_acl::apply_workspace_acls;

/// Bootstraps an elevated runner session for `plan` and returns the IPC pipe
/// pair `(write, read)` once the runner has acknowledged the spawn request.
pub(crate) fn start_runner_session(
    plan: &WindowsExecPlan,
    stdin_open: bool,
    timeout: Option<Duration>,
) -> Result<(File, File)> {
    let home = zagens_home_from_env();
    let caps = load_or_create_cap_sids(&home)?;
    let cap_sid = LocalSid::from_string(&caps.workspace)?;
    apply_workspace_acls(&plan.writable_roots, &plan.protected_write_paths, &cap_sid)?;

    let sandbox_creds = require_sandbox_creds(&home, plan.network_allowed)?;
    let logs_base = sandbox_dir(&home);

    let spawn_request = SpawnRequest {
        command: plan.argv.clone(),
        cwd: plan.cwd.clone(),
        env: plan.env.clone(),
        cap_sids: vec![caps.workspace.clone()],
        timeout_ms: timeout.map(|d| d.as_millis() as u64),
        tty: false,
        stdin_open,
    };

    let transport = spawn_runner_transport(
        &home,
        &plan.cwd,
        &sandbox_creds,
        Some(logs_base.as_path()),
        spawn_request,
    )
    .context("elevated runner transport")?;

    Ok(transport.into_files())
}
