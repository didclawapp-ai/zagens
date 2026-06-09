//! Elevated sandbox setup orchestration (UAC helper launch).

use std::ffi::c_void;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Security::{
    AllocateAndInitializeSid, CheckTokenMembership, FreeSid, SECURITY_NT_AUTHORITY,
};

use crate::logging::log_note;
use crate::paths::{sandbox_dir, sandbox_users_path, setup_marker_path, zagens_home_from_env};
use crate::setup_error::{
    SetupErrorCode, clear_setup_error_report, extract_setup_failure, failure,
    read_setup_error_report,
};

pub const SETUP_VERSION: u32 = 1;
pub const OFFLINE_USERNAME: &str = "ZagensSandboxOffline";
pub const ONLINE_USERNAME: &str = "ZagensSandboxOnline";
use crate::helpers::find_setup_exe;
use crate::winutil::quote_windows_arg;

const ERROR_CANCELLED: u32 = 1223;
const SECURITY_BUILTIN_DOMAIN_RID: u32 = 0x0000_0020;
const DOMAIN_ALIAS_RID_ADMINS: u32 = 0x0000_0220;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupMarker {
    pub version: u32,
    pub offline_username: String,
    pub online_username: String,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub proxy_ports: Vec<u16>,
    #[serde(default)]
    pub allow_local_binding: bool,
}

impl SetupMarker {
    pub fn version_matches(&self) -> bool {
        self.version == SETUP_VERSION
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxUserRecord {
    pub username: String,
    /// DPAPI-encrypted password blob, base64 encoded.
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxUsersFile {
    pub version: u32,
    pub offline: SandboxUserRecord,
    pub online: SandboxUserRecord,
}

impl SandboxUsersFile {
    pub fn version_matches(&self) -> bool {
        self.version == SETUP_VERSION
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SetupMode {
    #[default]
    Full,
    ProvisionOnly,
    /// Reverse-order cleanup of every elevated setup side effect (design §8.5).
    Teardown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevationPayload {
    pub version: u32,
    pub offline_username: String,
    pub online_username: String,
    pub zagens_home: PathBuf,
    pub real_user: String,
    #[serde(default)]
    pub mode: SetupMode,
    /// Profile directory of the real user (for grant-read planning); the
    /// elevated helper cannot derive it from its own environment.
    #[serde(default)]
    pub real_user_profile: Option<PathBuf>,
}

fn is_elevated() -> Result<bool> {
    unsafe {
        let mut administrators_group: *mut c_void = std::ptr::null_mut();
        let ok = AllocateAndInitializeSid(
            &SECURITY_NT_AUTHORITY,
            2,
            SECURITY_BUILTIN_DOMAIN_RID,
            DOMAIN_ALIAS_RID_ADMINS,
            0,
            0,
            0,
            0,
            0,
            0,
            &mut administrators_group,
        );
        if ok == 0 {
            return Err(anyhow::anyhow!(
                "AllocateAndInitializeSid failed: {}",
                GetLastError()
            ));
        }
        let mut is_member = 0i32;
        let check = CheckTokenMembership(0, administrators_group, &mut is_member as *mut _);
        FreeSid(administrators_group as *mut _);
        if check == 0 {
            return Err(anyhow::anyhow!(
                "CheckTokenMembership failed: {}",
                GetLastError()
            ));
        }
        Ok(is_member != 0)
    }
}

fn report_helper_failure(
    zagens_home: &Path,
    cleared_report: bool,
    exit_code: Option<i32>,
) -> anyhow::Error {
    let exit_detail = format!("setup helper exited with status {exit_code:?}");
    if !cleared_report {
        return failure(SetupErrorCode::OrchestratorHelperExitNonzero, exit_detail);
    }
    match read_setup_error_report(zagens_home) {
        Ok(Some(report)) => {
            anyhow::Error::new(crate::setup_error::SetupFailure::from_report(report))
        }
        Ok(None) => failure(SetupErrorCode::OrchestratorHelperExitNonzero, exit_detail),
        Err(err) => failure(
            SetupErrorCode::OrchestratorHelperReportReadFailed,
            format!("{exit_detail}; failed to read setup_error.json: {err}"),
        ),
    }
}

fn verify_setup_completed(zagens_home: &Path) -> Result<()> {
    if sandbox_setup_is_complete(zagens_home) {
        Ok(())
    } else {
        Err(failure(
            SetupErrorCode::OrchestratorHelperIncomplete,
            "setup helper exited successfully before setup completed",
        ))
    }
}

fn verify_teardown_completed(zagens_home: &Path) -> Result<()> {
    if sandbox_setup_artifacts_present(zagens_home) {
        Err(failure(
            SetupErrorCode::OrchestratorHelperIncomplete,
            "teardown helper exited successfully but setup artifacts remain",
        ))
    } else {
        Ok(())
    }
}

fn verify_helper_outcome(payload: &ElevationPayload, zagens_home: &Path) -> Result<()> {
    match payload.mode {
        SetupMode::Teardown => verify_teardown_completed(zagens_home),
        SetupMode::Full | SetupMode::ProvisionOnly => verify_setup_completed(zagens_home),
    }
}

fn run_setup_exe(
    payload: &ElevationPayload,
    needs_elevation: bool,
    zagens_home: &Path,
) -> Result<()> {
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, INFINITE, WaitForSingleObject,
    };
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };

    let sbx_dir = sandbox_dir(zagens_home);
    let exe = find_setup_exe(zagens_home, Some(&sbx_dir));
    let payload_json = serde_json::to_string(payload).map_err(|err| {
        failure(
            SetupErrorCode::OrchestratorPayloadSerializeFailed,
            format!("failed to serialize elevation payload: {err}"),
        )
    })?;
    let payload_b64 = BASE64_STANDARD.encode(payload_json.as_bytes());
    let cleared_report = match clear_setup_error_report(zagens_home) {
        Ok(()) => true,
        Err(err) => {
            log_note(
                &format!(
                    "setup orchestrator: failed to clear setup_error.json before launch: {err}"
                ),
                Some(&sbx_dir),
            );
            false
        }
    };

    if !needs_elevation {
        let status = Command::new(&exe)
            .arg(&payload_b64)
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|err| {
                failure(
                    SetupErrorCode::OrchestratorHelperLaunchFailed,
                    format!("failed to launch setup helper (non-elevated): {err}"),
                )
            })?;
        if !status.success() {
            return Err(report_helper_failure(
                zagens_home,
                cleared_report,
                status.code(),
            ));
        }
        verify_helper_outcome(payload, zagens_home)?;
        let _ = clear_setup_error_report(zagens_home);
        return Ok(());
    }

    let exe_w = crate::winutil::to_wide(&exe);
    let params = quote_windows_arg(&payload_b64);
    let params_w = crate::winutil::to_wide(&params);
    let verb_w = crate::winutil::to_wide("runas");
    let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    sei.fMask = SEE_MASK_NOCLOSEPROCESS;
    sei.lpVerb = verb_w.as_ptr();
    sei.lpFile = exe_w.as_ptr();
    sei.lpParameters = params_w.as_ptr();
    sei.nShow = 0;
    let ok = unsafe { ShellExecuteExW(&mut sei) };
    if ok == 0 || sei.hProcess == 0 {
        let last_error = unsafe { GetLastError() };
        let code = if last_error == ERROR_CANCELLED {
            SetupErrorCode::OrchestratorHelperLaunchCanceled
        } else {
            SetupErrorCode::OrchestratorHelperLaunchFailed
        };
        return Err(failure(
            code,
            format!("ShellExecuteExW failed to launch setup helper: {last_error}"),
        ));
    }
    unsafe {
        WaitForSingleObject(sei.hProcess, INFINITE);
        let mut exit_code: u32 = 0;
        GetExitCodeProcess(sei.hProcess, &mut exit_code);
        windows_sys::Win32::Foundation::CloseHandle(sei.hProcess);
        if exit_code != 0 {
            return Err(report_helper_failure(
                zagens_home,
                cleared_report,
                Some(exit_code as i32),
            ));
        }
    }
    verify_helper_outcome(payload, zagens_home)?;
    let _ = clear_setup_error_report(zagens_home);
    Ok(())
}

fn run_helper_with_mode(zagens_home: &Path, real_user: &str, mode: SetupMode) -> Result<()> {
    let sbx_dir = sandbox_dir(zagens_home);
    std::fs::create_dir_all(&sbx_dir).map_err(|err| {
        failure(
            SetupErrorCode::OrchestratorSandboxDirCreateFailed,
            format!("failed to create sandbox dir {}: {err}", sbx_dir.display()),
        )
    })?;
    let needs_elevation = !is_elevated().map_err(|err| {
        failure(
            SetupErrorCode::OrchestratorElevationCheckFailed,
            format!("failed to determine elevation state: {err}"),
        )
    })?;
    let payload = ElevationPayload {
        version: SETUP_VERSION,
        offline_username: OFFLINE_USERNAME.to_string(),
        online_username: ONLINE_USERNAME.to_string(),
        zagens_home: zagens_home.to_path_buf(),
        real_user: real_user.to_string(),
        mode,
        real_user_profile: std::env::var("USERPROFILE").ok().map(PathBuf::from),
    };
    run_setup_exe(&payload, needs_elevation, zagens_home)
}

/// Run elevated provisioning setup (creates sandbox users + marker). Requires admin when
/// invoked directly; otherwise triggers UAC via `ShellExecuteExW runas`.
pub fn run_elevated_provisioning_setup(zagens_home: &Path, real_user: &str) -> Result<()> {
    run_helper_with_mode(zagens_home, real_user, SetupMode::ProvisionOnly)
}

/// Convenience entry using the default Zagens home directory.
pub fn run_elevated_provisioning_setup_default(real_user: &str) -> Result<()> {
    run_elevated_provisioning_setup(&zagens_home_from_env(), real_user)
}

/// Run the elevated teardown helper (reverse-order cleanup of WFP filters,
/// Winlogon hide entries, sandbox users, and DPAPI secrets — design §8.5).
pub fn run_elevated_teardown(zagens_home: &Path, real_user: &str) -> Result<()> {
    run_helper_with_mode(zagens_home, real_user, SetupMode::Teardown)
}

/// Convenience entry using the default Zagens home directory.
pub fn run_elevated_teardown_default(real_user: &str) -> Result<()> {
    run_elevated_teardown(&zagens_home_from_env(), real_user)
}

/// Outcome of [`run_setup_refresh`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupRefreshOutcome {
    /// On-disk artifacts already match [`SETUP_VERSION`]; nothing re-run.
    AlreadyCurrent,
    /// Setup was (re-)provisioned because artifacts were missing or stale.
    Refreshed,
}

/// Idempotent setup refresh (PR-2.8 / design §8.3): re-runs the elevated
/// provisioning helper only when the on-disk artifacts are missing or carry a
/// version other than [`SETUP_VERSION`]. Pass `force` to re-run regardless
/// (e.g. after WFP filter verification failed).
pub fn run_setup_refresh(
    zagens_home: &Path,
    real_user: &str,
    force: bool,
) -> Result<SetupRefreshOutcome> {
    if !force && sandbox_setup_is_complete(zagens_home) {
        return Ok(SetupRefreshOutcome::AlreadyCurrent);
    }
    run_elevated_provisioning_setup(zagens_home, real_user)?;
    Ok(SetupRefreshOutcome::Refreshed)
}

pub fn extract_setup_failure_message(err: &anyhow::Error) -> Option<(String, String)> {
    extract_setup_failure(err).map(|f| (f.code.as_str().to_string(), f.message.clone()))
}

/// Returns true when on-disk setup artifacts exist and match [`SETUP_VERSION`].
pub fn sandbox_setup_is_complete(zagens_home: &Path) -> bool {
    let marker_ok =
        matches!(load_marker(zagens_home), Ok(Some(marker)) if marker.version_matches());
    if !marker_ok {
        return false;
    }
    matches!(load_users(zagens_home), Ok(Some(users)) if users.version_matches())
}

/// Returns true when any elevated setup artifact exists on disk, regardless of
/// version — used to decide whether teardown needs the elevated helper and to
/// distinguish "never set up" from "stale version" for refresh messaging.
pub fn sandbox_setup_artifacts_present(zagens_home: &Path) -> bool {
    setup_marker_path(zagens_home).exists() || sandbox_users_path(zagens_home).exists()
}

fn load_marker(zagens_home: &Path) -> Result<Option<SetupMarker>> {
    let path = setup_marker_path(zagens_home);
    match fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str::<SetupMarker>(&contents) {
            Ok(m) => Ok(Some(m)),
            Err(err) => {
                log_note(
                    &format!("sandbox setup marker parse failed: {err}"),
                    Some(&sandbox_dir(zagens_home)),
                );
                Ok(None)
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => {
            log_note(
                &format!("sandbox setup marker read failed: {err}"),
                Some(&sandbox_dir(zagens_home)),
            );
            Ok(None)
        }
    }
}

fn load_users(zagens_home: &Path) -> Result<Option<SandboxUsersFile>> {
    let path = sandbox_users_path(zagens_home);
    let file = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            log_note(
                &format!("sandbox users read failed: {err}"),
                Some(&sandbox_dir(zagens_home)),
            );
            return Ok(None);
        }
    };
    match serde_json::from_str::<SandboxUsersFile>(&file) {
        Ok(users) => Ok(Some(users)),
        Err(err) => {
            log_note(
                &format!("sandbox users parse failed: {err}"),
                Some(&sandbox_dir(zagens_home)),
            );
            Ok(None)
        }
    }
}
