//! Windows native sandbox for Zagens (`exec_shell` OS-level isolation).
//!
//! Design: `doc_Private/docs/tech/WINDOWS_SANDBOX_DESIGN.md`

#[cfg(not(windows))]
mod stub;

#[cfg(windows)]
mod acl;
#[cfg(windows)]
mod cap;
#[cfg(windows)]
mod deny_read;
#[cfg(windows)]
mod env;
#[cfg(windows)]
mod paths;
#[cfg(windows)]
mod plan;
#[cfg(windows)]
mod poc;
#[cfg(windows)]
mod process;
#[cfg(windows)]
mod ssh_config_dependencies;
#[cfg(windows)]
mod teardown;
#[cfg(windows)]
mod token;
#[cfg(windows)]
mod unelevated;
#[cfg(windows)]
mod winutil;
#[cfg(windows)]
mod workspace_acl;

#[cfg(not(windows))]
pub use stub::*;

#[cfg(windows)]
pub use deny_read::unelevated_deny_read_enabled;
#[cfg(windows)]
pub use plan::{
    PlanInput, WindowsExecPlan, WindowsSandboxMode, plan_exec, protected_subdirs_for_root,
};
#[cfg(windows)]
pub use poc::{UnelevatedDenyReadPocResult, run_unelevated_deny_read_poc, write_poc_result};
#[cfg(windows)]
pub use process::{CapturedOutput, ManagedProcess, SpawnStdio};
#[cfg(windows)]
pub use ssh_config_dependencies::filter_ssh_config_dependency_roots;
#[cfg(windows)]
pub use teardown::{TeardownReport, teardown_unelevated};
#[cfg(windows)]
pub use unelevated::{spawn, spawn_sync};

#[cfg(windows)]
pub fn is_enforcement_available() -> bool {
    cap::load_or_create_cap_sids(&paths::zagens_home_from_env()).is_ok()
}
