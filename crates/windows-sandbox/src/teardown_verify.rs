//! Post-teardown residual inspection for elevated sandbox (Gate G2 / design §15).

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::paths::{sandbox_secrets_dir, setup_marker_path};
use crate::setup::{OFFLINE_USERNAME, ONLINE_USERNAME, sandbox_setup_artifacts_present};
use crate::wfp::zagens_wfp_namespace_present;

/// Residual state after elevated teardown — all flags should be false for a clean uninstall.
#[derive(Debug, Clone, Serialize)]
pub struct ElevatedTeardownResidualReport {
    pub setup_marker_present: bool,
    pub secrets_dir_present: bool,
    pub setup_artifacts_present: bool,
    pub wfp_namespace_present: bool,
    pub offline_user_exists: bool,
    pub online_user_exists: bool,
    pub clean: bool,
}

/// Inspects on-disk setup artifacts, WFP namespace, and sandbox local users.
///
/// Does not mutate system state. Safe to call from acceptance examples after
/// [`crate::run_elevated_teardown`].
pub fn inspect_elevated_teardown_residuals(
    zagens_home: &Path,
) -> Result<ElevatedTeardownResidualReport> {
    let setup_marker_present = setup_marker_path(zagens_home).exists();
    let secrets_dir_present = sandbox_secrets_dir(zagens_home).exists();
    let setup_artifacts_present = sandbox_setup_artifacts_present(zagens_home);
    let wfp_namespace_present = zagens_wfp_namespace_present()?;
    let offline_user_exists = local_user_exists(OFFLINE_USERNAME);
    let online_user_exists = local_user_exists(ONLINE_USERNAME);
    let clean = !setup_marker_present
        && !secrets_dir_present
        && !setup_artifacts_present
        && !wfp_namespace_present
        && !offline_user_exists
        && !online_user_exists;
    Ok(ElevatedTeardownResidualReport {
        setup_marker_present,
        secrets_dir_present,
        setup_artifacts_present,
        wfp_namespace_present,
        offline_user_exists,
        online_user_exists,
        clean,
    })
}

fn local_user_exists(username: &str) -> bool {
    use std::ffi::OsStr;
    use std::ptr::null_mut;

    use windows_sys::Win32::NetworkManagement::NetManagement::{
        NERR_Success, NERR_UserNotFound, NetApiBufferFree, NetUserGetInfo,
    };

    let name_w = crate::winutil::to_wide(OsStr::new(username));
    let mut buffer = null_mut();
    let status = unsafe { NetUserGetInfo(null_mut(), name_w.as_ptr(), 0, &mut buffer) };
    if status == NERR_Success {
        unsafe {
            NetApiBufferFree(buffer as *const _);
        }
        true
    } else {
        status != NERR_UserNotFound
    }
}
