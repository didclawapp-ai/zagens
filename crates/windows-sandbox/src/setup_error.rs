//! Structured elevated-setup failures (Codex-aligned error codes).

use anyhow::Context;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::paths::sandbox_dir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupErrorCode {
    OrchestratorSandboxDirCreateFailed,
    OrchestratorElevationCheckFailed,
    OrchestratorElevationRequired,
    OrchestratorPayloadSerializeFailed,
    OrchestratorHelperLaunchFailed,
    OrchestratorHelperLaunchCanceled,
    OrchestratorHelperExitNonzero,
    OrchestratorHelperReportReadFailed,
    OrchestratorHelperIncomplete,
    HelperRequestArgsFailed,
    HelperSandboxDirCreateFailed,
    HelperLogFailed,
    HelperUserProvisionFailed,
    HelperUsersGroupCreateFailed,
    HelperUserCreateOrUpdateFailed,
    HelperDpapiProtectFailed,
    HelperUsersFileWriteFailed,
    HelperSetupMarkerWriteFailed,
    HelperSidResolveFailed,
    HelperSandboxLockFailed,
    HelperUnknownError,
}

impl SetupErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OrchestratorSandboxDirCreateFailed => "orchestrator_sandbox_dir_create_failed",
            Self::OrchestratorElevationCheckFailed => "orchestrator_elevation_check_failed",
            Self::OrchestratorElevationRequired => "orchestrator_elevation_required",
            Self::OrchestratorPayloadSerializeFailed => "orchestrator_payload_serialize_failed",
            Self::OrchestratorHelperLaunchFailed => "orchestrator_helper_launch_failed",
            Self::OrchestratorHelperLaunchCanceled => "orchestrator_helper_launch_canceled",
            Self::OrchestratorHelperExitNonzero => "orchestrator_helper_exit_nonzero",
            Self::OrchestratorHelperReportReadFailed => "orchestrator_helper_report_read_failed",
            Self::OrchestratorHelperIncomplete => "orchestrator_helper_incomplete",
            Self::HelperRequestArgsFailed => "helper_request_args_failed",
            Self::HelperSandboxDirCreateFailed => "helper_sandbox_dir_create_failed",
            Self::HelperLogFailed => "helper_log_failed",
            Self::HelperUserProvisionFailed => "helper_user_provision_failed",
            Self::HelperUsersGroupCreateFailed => "helper_users_group_create_failed",
            Self::HelperUserCreateOrUpdateFailed => "helper_user_create_or_update_failed",
            Self::HelperDpapiProtectFailed => "helper_dpapi_protect_failed",
            Self::HelperUsersFileWriteFailed => "helper_users_file_write_failed",
            Self::HelperSetupMarkerWriteFailed => "helper_setup_marker_write_failed",
            Self::HelperSidResolveFailed => "helper_sid_resolve_failed",
            Self::HelperSandboxLockFailed => "helper_sandbox_lock_failed",
            Self::HelperUnknownError => "helper_unknown_error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupErrorReport {
    pub code: SetupErrorCode,
    pub message: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SetupFailure {
    pub code: SetupErrorCode,
    pub message: String,
}

impl SetupFailure {
    pub fn new(code: SetupErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn from_report(report: SetupErrorReport) -> Self {
        Self::new(report.code, report.message)
    }
}

impl std::fmt::Display for SetupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for SetupFailure {}

pub fn failure(code: SetupErrorCode, message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(SetupFailure::new(code, message))
}

pub fn extract_setup_failure(err: &anyhow::Error) -> Option<&SetupFailure> {
    err.downcast_ref::<SetupFailure>()
}

pub fn setup_error_path(zagens_home: &Path) -> PathBuf {
    sandbox_dir(zagens_home).join("setup_error.json")
}

pub fn clear_setup_error_report(zagens_home: &Path) -> Result<()> {
    let path = setup_error_path(zagens_home);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("remove {}", path.display())),
    }
}

pub fn write_setup_error_report(zagens_home: &Path, report: &SetupErrorReport) -> Result<()> {
    let dir = sandbox_dir(zagens_home);
    fs::create_dir_all(&dir).with_context(|| format!("create sandbox dir {}", dir.display()))?;
    let path = setup_error_path(zagens_home);
    let json = serde_json::to_vec_pretty(report)?;
    fs::write(&path, json).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

pub fn read_setup_error_report(zagens_home: &Path) -> Result<Option<SetupErrorReport>> {
    let path = setup_error_path(zagens_home);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("read {}", path.display())),
    };
    let report = serde_json::from_slice::<SetupErrorReport>(&bytes)
        .with_context(|| format!("parse {}", path.display()))?;
    Ok(Some(report))
}
