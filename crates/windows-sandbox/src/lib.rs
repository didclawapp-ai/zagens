//! Windows native sandbox for Zagens (`exec_shell` OS-level isolation).
//!
//! Design: `doc_Private/docs/tech/WINDOWS_SANDBOX_DESIGN.md`

#![allow(unsafe_op_in_unsafe_fn)]

#[cfg(not(windows))]
mod stub;

#[cfg(windows)]
mod acl;
#[cfg(windows)]
mod audit;
#[cfg(windows)]
mod cap;
#[cfg(windows)]
mod conpty;
#[cfg(windows)]
mod deny_read;
#[cfg(windows)]
mod deny_read_state;
#[cfg(windows)]
mod dpapi;
#[cfg(windows)]
mod elevated;
#[cfg(windows)]
mod env;
#[cfg(windows)]
mod grant_read;
#[cfg(windows)]
mod helper_materialization;
#[cfg(windows)]
mod helpers;
#[cfg(windows)]
mod hide_users;
#[cfg(windows)]
mod identity_creds;
#[cfg(windows)]
mod logging;
#[cfg(windows)]
mod paths;
#[cfg(windows)]
mod plan;
#[cfg(windows)]
mod poc;
#[cfg(windows)]
mod private_desktop;
#[cfg(windows)]
mod process;
#[cfg(windows)]
mod process_startup;
#[cfg(windows)]
mod setup;
#[cfg(windows)]
mod setup_error;
#[cfg(windows)]
mod spawn;
#[cfg(windows)]
mod ssh_config_dependencies;
#[cfg(windows)]
mod teardown;
#[cfg(windows)]
mod teardown_verify;
#[cfg(windows)]
mod token;
#[cfg(windows)]
mod unelevated;
#[cfg(windows)]
mod wfp;
#[cfg(windows)]
pub mod wfp_setup;
#[cfg(windows)]
mod winutil;
#[cfg(windows)]
mod workspace_acl;

#[cfg(not(windows))]
pub use stub::*;

#[cfg(windows)]
pub use audit::{
    AUDIT_TIME_BUDGET, AuditReport, deny_write_on_offenders, scan_everyone_writable,
    write_audit_report,
};
#[cfg(windows)]
pub use deny_read::unelevated_deny_read_enabled;
#[cfg(windows)]
pub use deny_read_state::{revoke_elevated_deny_read, sync_elevated_deny_read};
#[cfg(windows)]
pub use dpapi::{protect as dpapi_protect, unprotect as dpapi_unprotect};
#[cfg(windows)]
pub use elevated::{
    ElevatedChild, ErrorPayload, ExitPayload, FramedMessage, IPC_PROTOCOL_VERSION, Message,
    OutputPayload, OutputStream, SpawnReady, SpawnRequest, decode_bytes, encode_bytes, read_frame,
    write_frame,
};
#[cfg(windows)]
pub use grant_read::{
    GrantReadReport, PROFILE_GRANT_TIME_BUDGET, SANDBOX_USERS_GROUP, add_session_read_dir,
    apply_profile_read_grants, revoke_read_grants, sandbox_users_group_sid,
    userprofile_root_exclusions,
};
#[cfg(windows)]
pub use hide_users::{hide_newly_created_users, unhide_removed_users};
#[cfg(windows)]
pub use logging::{log_note, log_writer};
#[cfg(windows)]
pub use paths::{
    sandbox_dir, sandbox_secrets_dir, sandbox_users_path, setup_marker_path, zagens_home,
    zagens_home_from_env,
};
#[cfg(windows)]
pub use plan::{
    PlanInput, WindowsExecPlan, WindowsSandboxMode, plan_exec, protected_subdirs_for_root,
};
#[cfg(windows)]
pub use poc::{UnelevatedDenyReadPocResult, run_unelevated_deny_read_poc, write_poc_result};
#[cfg(windows)]
pub use process::{
    CapturedOutput, ManagedProcess, SpawnDenial, SpawnOptions, SpawnStdio,
    extract_spawn_denial_code,
};
#[cfg(windows)]
pub use process::{read_handle_loop, spawn_with_stdio};
#[cfg(windows)]
pub use setup::{
    ElevationPayload, OFFLINE_USERNAME, ONLINE_USERNAME, SETUP_VERSION, SandboxUserRecord,
    SandboxUsersFile, SetupMarker, SetupMode, SetupRefreshOutcome, extract_setup_failure_message,
    run_elevated_provisioning_setup, run_elevated_provisioning_setup_default,
    run_elevated_teardown, run_elevated_teardown_default, run_setup_refresh,
    sandbox_setup_artifacts_present, sandbox_setup_is_complete,
};
#[cfg(windows)]
pub use setup_error::{
    SetupErrorCode, SetupErrorReport, SetupFailure, extract_setup_failure, write_setup_error_report,
};
#[cfg(windows)]
pub use spawn::{spawn, spawn_background_elevated, spawn_sync};
#[cfg(windows)]
pub use ssh_config_dependencies::filter_ssh_config_dependency_roots;
#[cfg(windows)]
pub use teardown::{TeardownReport, teardown_unelevated};
#[cfg(windows)]
pub use teardown_verify::{ElevatedTeardownResidualReport, inspect_elevated_teardown_residuals};
#[cfg(windows)]
pub use token::{LocalSid, create_restricted_token_with_capabilities};
#[cfg(windows)]
pub use wfp::zagens_wfp_namespace_present;
#[cfg(windows)]
pub use winutil::{resolve_sid, string_from_sid_bytes, to_wide};

#[cfg(windows)]
pub fn is_enforcement_available() -> bool {
    cap::load_or_create_cap_sids(&paths::zagens_home_from_env()).is_ok()
}
