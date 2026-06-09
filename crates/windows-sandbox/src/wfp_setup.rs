//! Best-effort WFP filter installation during elevated setup (non-fatal on failure).

use std::path::Path;

use crate::wfp::install_wfp_filters_for_account;
use crate::wfp::remove_wfp_filters;

fn panic_payload_to_string(panic_payload: Box<dyn std::any::Any + Send>) -> String {
    match panic_payload.downcast::<String>() {
        Ok(message) => *message,
        Err(panic_payload) => match panic_payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_string(),
            Err(_) => "unknown panic payload".to_string(),
        },
    }
}

/// Installs persistent outbound block filters for the offline sandbox account.
///
/// Failures are logged and do not abort the rest of setup provisioning.
pub fn install_wfp_filters<F>(_zagens_home: &Path, offline_username: &str, mut log: F)
where
    F: FnMut(&str),
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        install_wfp_filters_for_account(offline_username)
    })) {
        Ok(Ok(installed_filter_count)) => {
            log(&format!(
                "WFP setup succeeded for {offline_username} with {installed_filter_count} installed filters"
            ));
        }
        Ok(Err(err)) => {
            log(&format!(
                "WFP setup failed for {offline_username}: {err}; continuing elevated setup"
            ));
        }
        Err(panic_payload) => {
            let error = panic_payload_to_string(panic_payload);
            log(&format!(
                "WFP setup panicked for {offline_username}: {error}; continuing elevated setup"
            ));
        }
    }
}

/// Removes the persistent Zagens WFP filters, sublayer, and provider.
///
/// Returns `true` when the WFP namespace is clean afterwards. Failures are
/// logged so teardown can continue with the remaining cleanup steps.
pub fn uninstall_wfp_filters<F>(mut log: F) -> bool
where
    F: FnMut(&str),
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(remove_wfp_filters)) {
        Ok(Ok(removed_filter_count)) => {
            log(&format!(
                "WFP teardown succeeded: removed {removed_filter_count} filters plus sublayer/provider"
            ));
            true
        }
        Ok(Err(err)) => {
            log(&format!("WFP teardown failed: {err}; continuing teardown"));
            false
        }
        Err(panic_payload) => {
            let error = panic_payload_to_string(panic_payload);
            log(&format!(
                "WFP teardown panicked: {error}; continuing teardown"
            ));
            false
        }
    }
}
