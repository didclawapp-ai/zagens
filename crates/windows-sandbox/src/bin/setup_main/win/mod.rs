mod sandbox_users;

use std::io::Write;

use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Deserialize;
use zagens_windows_sandbox::{
    AUDIT_TIME_BUDGET, LocalSid, PROFILE_GRANT_TIME_BUDGET, SETUP_VERSION, SetupErrorCode,
    SetupErrorReport, SetupFailure, SetupMode, apply_profile_read_grants, extract_setup_failure,
    hide_newly_created_users, log_note, log_writer, resolve_sid, revoke_elevated_deny_read,
    revoke_read_grants, sandbox_dir, scan_everyone_writable, string_from_sid_bytes,
    sync_elevated_deny_read, unhide_removed_users, write_audit_report, write_setup_error_report,
};

use sandbox_users::{
    SANDBOX_USERS_GROUP, commit_setup_marker, delete_sandbox_group, delete_sandbox_users,
    provision_sandbox_users, remove_profile_dirs_best_effort, remove_setup_artifacts,
};

#[derive(Debug, Clone, Deserialize)]
struct Payload {
    version: u32,
    offline_username: String,
    online_username: String,
    zagens_home: std::path::PathBuf,
    real_user: String,
    #[serde(default)]
    mode: SetupMode,
    #[serde(default)]
    real_user_profile: Option<std::path::PathBuf>,
}

pub fn main() -> Result<()> {
    let ret = real_main();
    if let Err(e) = &ret {
        if let Ok(home) = std::env::var("ZAGENS_HOME").or_else(|_| std::env::var("DEEPSEEK_HOME")) {
            let sbx_dir = sandbox_dir(std::path::Path::new(&home));
            let _ = std::fs::create_dir_all(&sbx_dir);
            if let Some(mut f) = log_writer(&sbx_dir) {
                let _ = writeln!(
                    f,
                    "[{}] top-level error: {}",
                    chrono::Utc::now().to_rfc3339(),
                    e
                );
            }
        }
    }
    ret
}

fn real_main() -> Result<()> {
    let mut args = std::env::args().collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperRequestArgsFailed,
            "expected payload argument",
        )));
    }
    let payload_b64 = args.remove(1);
    let payload_json = BASE64.decode(payload_b64).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperRequestArgsFailed,
            format!("failed to decode payload b64: {err}"),
        ))
    })?;
    let payload: Payload = serde_json::from_slice(&payload_json).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperRequestArgsFailed,
            format!("failed to parse payload json: {err}"),
        ))
    })?;
    // Teardown intentionally tolerates version skew: cleanup must still run
    // when the on-disk artifacts came from an older or newer setup binary.
    if payload.version != SETUP_VERSION && payload.mode != SetupMode::Teardown {
        return Err(anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperRequestArgsFailed,
            format!(
                "setup version mismatch: expected {SETUP_VERSION}, got {}",
                payload.version
            ),
        )));
    }
    let sbx_dir = sandbox_dir(&payload.zagens_home);
    std::fs::create_dir_all(&sbx_dir).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSandboxDirCreateFailed,
            format!("failed to create sandbox dir {}: {err}", sbx_dir.display()),
        ))
    })?;
    let mut log = log_writer(&sbx_dir).ok_or_else(|| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperLogFailed,
            format!("open log in {} failed", sbx_dir.display()),
        ))
    })?;
    let result = run_setup(&payload, &mut log, &sbx_dir);
    if let Err(err) = &result {
        let _ = log_line(&mut log, &format!("setup error: {err:?}"));
        log_note(&format!("setup error: {err:?}"), Some(sbx_dir.as_path()));
        let failure = extract_setup_failure(err)
            .map(|f| SetupFailure::new(f.code, f.message.clone()))
            .unwrap_or_else(|| {
                SetupFailure::new(SetupErrorCode::HelperUnknownError, err.to_string())
            });
        let report = SetupErrorReport {
            code: failure.code,
            message: failure.message,
        };
        let _ = write_setup_error_report(&payload.zagens_home, &report);
    }
    result
}

fn run_setup(payload: &Payload, log: &mut dyn Write, sbx_dir: &std::path::Path) -> Result<()> {
    match payload.mode {
        SetupMode::Teardown => return run_teardown(payload, log, sbx_dir),
        SetupMode::ProvisionOnly | SetupMode::Full => run_provision_only(payload, log, sbx_dir),
    }?;
    commit_setup_marker(
        &payload.zagens_home,
        &payload.offline_username,
        &payload.online_username,
        &[],
        false,
    )?;
    Ok(())
}

fn run_provision_only(
    payload: &Payload,
    log: &mut dyn Write,
    sbx_dir: &std::path::Path,
) -> Result<()> {
    provision_sandbox_users(
        &payload.zagens_home,
        &payload.offline_username,
        &payload.online_username,
        log,
    )?;
    zagens_windows_sandbox::wfp_setup::install_wfp_filters(
        &payload.zagens_home,
        &payload.offline_username,
        |msg| {
            let _ = log_line(log, msg);
        },
    );
    apply_read_isolation_acls(payload, log);
    run_everyone_writable_audit(payload, log);
    hide_newly_created_users(
        &[
            payload.offline_username.clone(),
            payload.online_username.clone(),
        ],
        sbx_dir,
    );
    log_note("setup provisioning completed", Some(sbx_dir));
    Ok(())
}

/// Resolves the ZagensSandboxUsers group SID once the group exists.
fn sandbox_group_sid() -> Result<LocalSid> {
    let sid_bytes = resolve_sid(SANDBOX_USERS_GROUP)?;
    let sid_string = string_from_sid_bytes(&sid_bytes)
        .map_err(|err| anyhow::anyhow!("group SID stringify failed: {err}"))?;
    LocalSid::from_string(&sid_string)
}

fn payload_profile(payload: &Payload) -> std::path::PathBuf {
    payload.real_user_profile.clone().unwrap_or_else(|| {
        let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        std::path::Path::new(&system_drive)
            .join("Users")
            .join(&payload.real_user)
    })
}

/// PR-2.4 + PR-2.5: grant-exclusion profile read grants plus the explicit
/// deny-read backstop for the sandbox users group. Best-effort — failures are
/// logged and do not abort provisioning.
fn apply_read_isolation_acls(payload: &Payload, log: &mut dyn Write) {
    let group_sid = match sandbox_group_sid() {
        Ok(sid) => sid,
        Err(err) => {
            let _ = log_line(log, &format!("read isolation skipped: {err}"));
            return;
        }
    };
    let profile = payload_profile(payload);
    if !profile.is_dir() {
        let _ = log_line(
            log,
            &format!(
                "read isolation skipped: profile {} not found",
                profile.display()
            ),
        );
        return;
    }
    let mut messages = Vec::new();
    match apply_profile_read_grants(
        &payload.zagens_home,
        &profile,
        &group_sid,
        PROFILE_GRANT_TIME_BUDGET,
        |msg| messages.push(msg.to_string()),
    ) {
        Ok(report) => {
            let _ = log_line(
                log,
                &format!(
                    "grant-read: granted={} failed={} truncated={}",
                    report.granted, report.failed, report.truncated
                ),
            );
        }
        Err(err) => {
            let _ = log_line(log, &format!("grant-read failed: {err}"));
        }
    }
    match sync_elevated_deny_read(&payload.zagens_home, &profile, &group_sid, |msg| {
        messages.push(msg.to_string())
    }) {
        Ok(count) => {
            let _ = log_line(log, &format!("deny-read backstop: {count} path(s) pinned"));
        }
        Err(err) => {
            let _ = log_line(log, &format!("deny-read backstop failed: {err}"));
        }
    }
    for msg in messages {
        let _ = log_line(log, &msg);
    }
}

/// PR-2.7: warn about Everyone-writable directories (write-restricted tokens
/// cannot block writes there — design §13.6).
fn run_everyone_writable_audit(payload: &Payload, log: &mut dyn Write) {
    let mut roots = vec![payload_profile(payload)];
    if let Ok(temp) = std::env::var("TEMP") {
        roots.push(std::path::PathBuf::from(temp));
    }
    match scan_everyone_writable(&roots, AUDIT_TIME_BUDGET) {
        Ok(report) => {
            for path in &report.everyone_writable {
                let _ = log_line(
                    log,
                    &format!(
                        "audit warning: {} is Everyone-writable; sandbox write isolation does not cover it",
                        path.display()
                    ),
                );
            }
            let _ = log_line(
                log,
                &format!(
                    "audit: scanned={} everyone_writable={} truncated={}",
                    report.scanned,
                    report.everyone_writable.len(),
                    report.truncated
                ),
            );
            let _ = write_audit_report(&payload.zagens_home, &report);
        }
        Err(err) => {
            let _ = log_line(log, &format!("audit scan failed: {err}"));
        }
    }
}

/// Reverse-order elevated teardown (PR-2.9 / design §8.5):
/// read-grant + deny-read ACE revoke → WFP filters/sublayer/provider →
/// Winlogon UserList → local users → group → secrets/marker → profile dirs.
fn run_teardown(payload: &Payload, log: &mut dyn Write, sbx_dir: &std::path::Path) -> Result<()> {
    if let Ok(group_sid) = sandbox_group_sid() {
        let revoked_reads = revoke_read_grants(&payload.zagens_home, &group_sid);
        let revoked_denies = revoke_elevated_deny_read(&payload.zagens_home, &group_sid);
        let _ = log_line(
            log,
            &format!(
                "teardown: revoked {revoked_reads} read grant(s), {revoked_denies} deny-read ACE(s)"
            ),
        );
    } else {
        let _ = log_line(log, "teardown: sandbox group missing; ACL revoke skipped");
    }

    zagens_windows_sandbox::wfp_setup::uninstall_wfp_filters(|msg| {
        let _ = log_line(log, msg);
    });

    let usernames = [
        payload.offline_username.clone(),
        payload.online_username.clone(),
    ];
    unhide_removed_users(&usernames, sbx_dir);

    delete_sandbox_users(
        &[
            payload.offline_username.as_str(),
            payload.online_username.as_str(),
        ],
        log,
    )?;
    delete_sandbox_group(log)?;
    remove_setup_artifacts(&payload.zagens_home)?;
    remove_profile_dirs_best_effort(
        &[
            payload.offline_username.as_str(),
            payload.online_username.as_str(),
        ],
        log,
    );

    log_note("teardown completed", Some(sbx_dir));
    Ok(())
}

fn log_line(log: &mut dyn Write, msg: &str) -> Result<()> {
    let ts = chrono::Utc::now().to_rfc3339();
    writeln!(log, "[{ts}] {msg}")?;
    Ok(())
}
