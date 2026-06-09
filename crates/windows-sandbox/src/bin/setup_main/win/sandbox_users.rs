use std::ffi::{OsStr, c_void};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rand::{RngCore, SeedableRng, rngs::SmallRng};
use serde::Serialize;
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, GetLastError};
use windows_sys::Win32::NetworkManagement::NetManagement::{
    LOCALGROUP_INFO_1, LOCALGROUP_MEMBERS_INFO_3, NERR_Success, NetLocalGroupAdd,
    NetLocalGroupAddMembers, NetLocalGroupDel, NetUserAdd, NetUserDel, NetUserSetInfo,
    UF_DONT_EXPIRE_PASSWD, UF_SCRIPT, USER_INFO_1, USER_INFO_1003, USER_PRIV_USER,
};
use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows_sys::Win32::Security::{
    CopySid, GetLengthSid, LookupAccountNameW, LookupAccountSidW, SID_NAME_USE,
};

use zagens_windows_sandbox::{
    SETUP_VERSION, SetupErrorCode, SetupFailure, dpapi_protect, sandbox_dir, sandbox_secrets_dir,
    setup_marker_path, to_wide,
};

pub const SANDBOX_USERS_GROUP: &str = "ZagensSandboxUsers";
const SANDBOX_USERS_GROUP_COMMENT: &str = "Zagens sandbox internal group (managed)";

/// BUILTIN\Users well-known SID; resolved to the localized group name at
/// runtime so membership works on non-English Windows.
const BUILTIN_USERS_SID: &str = "S-1-5-32-545";

const NERR_USER_NOT_FOUND: u32 = 2221;
const NERR_GROUP_NOT_FOUND: u32 = 2220;
const ERROR_NO_SUCH_ALIAS: u32 = 1376;

pub fn provision_sandbox_users(
    zagens_home: &Path,
    offline_username: &str,
    online_username: &str,
    log: &mut dyn Write,
) -> Result<()> {
    ensure_sandbox_users_group(log)?;
    log_line(
        log,
        &format!("ensuring sandbox users offline={offline_username} online={online_username}"),
    )?;
    let offline_password = random_password();
    let online_password = random_password();
    ensure_sandbox_user(offline_username, &offline_password, log)?;
    ensure_sandbox_user(online_username, &online_password, log)?;
    write_secrets(
        zagens_home,
        offline_username,
        &offline_password,
        online_username,
        &online_password,
    )?;
    Ok(())
}

fn ensure_sandbox_users_group(log: &mut dyn Write) -> Result<()> {
    ensure_local_group(SANDBOX_USERS_GROUP, SANDBOX_USERS_GROUP_COMMENT, log)
}

fn ensure_sandbox_user(username: &str, password: &str, log: &mut dyn Write) -> Result<()> {
    ensure_local_user(username, password, log)?;
    ensure_local_group_member(SANDBOX_USERS_GROUP, username)?;
    // `NetUserAdd` does not grant BUILTIN\Users membership by itself; without
    // it the sandbox account lacks the standard system read permissions
    // (Windows/Program Files) that the grant-exclusion model assumes.
    match localized_builtin_users_group() {
        Ok(group) => {
            let _ = ensure_local_group_member(&group, username);
        }
        Err(err) => {
            log_line(
                log,
                &format!("BUILTIN\\Users lookup failed for {username}: {err}; skipping"),
            )?;
        }
    }
    Ok(())
}

/// Resolves the localized name of BUILTIN\Users via its well-known SID.
fn localized_builtin_users_group() -> Result<String> {
    let sid = sid_bytes_from_string(BUILTIN_USERS_SID)?;
    let mut name: Vec<u16> = vec![0; 64];
    let mut name_len = name.len() as u32;
    let mut domain: Vec<u16> = vec![0; 64];
    let mut domain_len = domain.len() as u32;
    let mut use_type: SID_NAME_USE = 0;
    loop {
        let ok = unsafe {
            LookupAccountSidW(
                std::ptr::null(),
                sid.as_ptr() as *mut c_void,
                name.as_mut_ptr(),
                &mut name_len,
                domain.as_mut_ptr(),
                &mut domain_len,
                &mut use_type,
            )
        };
        if ok != 0 {
            return Ok(String::from_utf16_lossy(&name[..name_len as usize]));
        }
        let err = unsafe { GetLastError() };
        if err == ERROR_INSUFFICIENT_BUFFER {
            name.resize(name_len as usize, 0);
            domain.resize(domain_len as usize, 0);
            continue;
        }
        return Err(anyhow::anyhow!(
            "LookupAccountSidW failed for {BUILTIN_USERS_SID}: {err}"
        ));
    }
}

/// Deletes the sandbox local users (teardown step 4); missing users are fine.
pub fn delete_sandbox_users(usernames: &[&str], log: &mut dyn Write) -> Result<()> {
    for username in usernames {
        let name_w = to_wide(OsStr::new(*username));
        let status = unsafe { NetUserDel(std::ptr::null(), name_w.as_ptr()) };
        if status != NERR_Success && status != NERR_USER_NOT_FOUND {
            log_line(
                log,
                &format!("NetUserDel failed for {username} code {status}"),
            )?;
            return Err(anyhow::Error::new(SetupFailure::new(
                SetupErrorCode::HelperUserCreateOrUpdateFailed,
                format!("failed to delete sandbox user {username}, code {status}"),
            )));
        }
    }
    Ok(())
}

/// Deletes the Zagens sandbox local group (teardown); missing group is fine.
pub fn delete_sandbox_group(log: &mut dyn Write) -> Result<()> {
    let name_w = to_wide(OsStr::new(SANDBOX_USERS_GROUP));
    let status = unsafe { NetLocalGroupDel(std::ptr::null(), name_w.as_ptr()) };
    if status != NERR_Success && status != NERR_GROUP_NOT_FOUND && status != ERROR_NO_SUCH_ALIAS {
        log_line(
            log,
            &format!("NetLocalGroupDel failed for {SANDBOX_USERS_GROUP} code {status}"),
        )?;
        return Err(anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperUsersGroupCreateFailed,
            format!("failed to delete local group {SANDBOX_USERS_GROUP}, code {status}"),
        )));
    }
    Ok(())
}

/// Removes the DPAPI secrets directory and the setup marker (teardown step 5).
pub fn remove_setup_artifacts(zagens_home: &Path) -> Result<()> {
    let secrets_dir = sandbox_secrets_dir(zagens_home);
    if secrets_dir.exists() {
        std::fs::remove_dir_all(&secrets_dir).map_err(|err| {
            anyhow::Error::new(SetupFailure::new(
                SetupErrorCode::HelperUsersFileWriteFailed,
                format!(
                    "failed to remove secrets dir {}: {err}",
                    secrets_dir.display()
                ),
            ))
        })?;
    }
    let marker_path = setup_marker_path(zagens_home);
    match std::fs::remove_file(&marker_path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSetupMarkerWriteFailed,
            format!(
                "failed to remove setup marker {}: {err}",
                marker_path.display()
            ),
        ))),
    }
}

/// Best-effort removal of `C:\Users\ZagensSandbox*` profile directories
/// (teardown step 6); locked or missing profiles are skipped with a log line.
pub fn remove_profile_dirs_best_effort(usernames: &[&str], log: &mut dyn Write) {
    let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    for username in usernames {
        let profile = Path::new(&system_drive).join("Users").join(username);
        if !profile.exists() {
            continue;
        }
        if let Err(err) = std::fs::remove_dir_all(&profile) {
            let _ = log_line(
                log,
                &format!(
                    "profile dir cleanup skipped for {}: {err}",
                    profile.display()
                ),
            );
        }
    }
}

fn ensure_local_user(name: &str, password: &str, log: &mut dyn Write) -> Result<()> {
    let name_w = to_wide(OsStr::new(name));
    let pwd_w = to_wide(OsStr::new(password));
    unsafe {
        let info = USER_INFO_1 {
            usri1_name: name_w.as_ptr() as *mut u16,
            usri1_password: pwd_w.as_ptr() as *mut u16,
            usri1_password_age: 0,
            usri1_priv: USER_PRIV_USER,
            usri1_home_dir: std::ptr::null_mut(),
            usri1_comment: std::ptr::null_mut(),
            usri1_flags: UF_SCRIPT | UF_DONT_EXPIRE_PASSWD,
            usri1_script_path: std::ptr::null_mut(),
        };
        let status = NetUserAdd(
            std::ptr::null(),
            1,
            &info as *const _ as *mut u8,
            std::ptr::null_mut(),
        );
        if status != NERR_Success {
            let pw_info = USER_INFO_1003 {
                usri1003_password: pwd_w.as_ptr() as *mut u16,
            };
            let upd = NetUserSetInfo(
                std::ptr::null(),
                name_w.as_ptr(),
                1003,
                &pw_info as *const _ as *mut u8,
                std::ptr::null_mut(),
            );
            if upd != NERR_Success {
                log_line(log, &format!("NetUserSetInfo failed for {name} code {upd}"))?;
                return Err(anyhow::Error::new(SetupFailure::new(
                    SetupErrorCode::HelperUserCreateOrUpdateFailed,
                    format!("failed to create/update user {name}, code {status}/{upd}"),
                )));
            }
        }
    }
    Ok(())
}

fn ensure_local_group(name: &str, comment: &str, log: &mut dyn Write) -> Result<()> {
    const ERROR_ALIAS_EXISTS: u32 = 1379;
    const NERR_GROUP_EXISTS: u32 = 2223;

    let name_w = to_wide(OsStr::new(name));
    let comment_w = to_wide(OsStr::new(comment));
    unsafe {
        let info = LOCALGROUP_INFO_1 {
            lgrpi1_name: name_w.as_ptr() as *mut u16,
            lgrpi1_comment: comment_w.as_ptr() as *mut u16,
        };
        let mut parm_err: u32 = 0;
        let status = NetLocalGroupAdd(
            std::ptr::null(),
            1,
            &info as *const _ as *mut u8,
            &mut parm_err as *mut _,
        );
        if status != NERR_Success && status != ERROR_ALIAS_EXISTS && status != NERR_GROUP_EXISTS {
            log_line(
                log,
                &format!("NetLocalGroupAdd failed for {name} code {status} parm_err={parm_err}"),
            )?;
            return Err(anyhow::Error::new(SetupFailure::new(
                SetupErrorCode::HelperUsersGroupCreateFailed,
                format!("failed to create local group {name}, code {status}"),
            )));
        }
    }
    Ok(())
}

fn ensure_local_group_member(group_name: &str, member_name: &str) -> Result<()> {
    let group_w = to_wide(OsStr::new(group_name));
    let member_w = to_wide(OsStr::new(member_name));
    unsafe {
        let member = LOCALGROUP_MEMBERS_INFO_3 {
            lgrmi3_domainandname: member_w.as_ptr() as *mut u16,
        };
        let _ = NetLocalGroupAddMembers(
            std::ptr::null(),
            group_w.as_ptr(),
            3,
            &member as *const _ as *mut u8,
            1,
        );
    }
    Ok(())
}

#[derive(Serialize)]
struct SandboxUserRecord {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct SandboxUsersFile {
    version: u32,
    offline: SandboxUserRecord,
    online: SandboxUserRecord,
}

#[derive(Serialize)]
struct SetupMarker {
    version: u32,
    offline_username: String,
    online_username: String,
    created_at: String,
    proxy_ports: Vec<u16>,
    allow_local_binding: bool,
}

fn write_secrets(
    zagens_home: &Path,
    offline_user: &str,
    offline_pwd: &str,
    online_user: &str,
    online_pwd: &str,
) -> Result<()> {
    let secrets_dir = sandbox_secrets_dir(zagens_home);
    std::fs::create_dir_all(&secrets_dir).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperUsersFileWriteFailed,
            format!(
                "failed to create secrets dir {}: {err}",
                secrets_dir.display()
            ),
        ))
    })?;
    let offline_blob = dpapi_protect(offline_pwd.as_bytes()).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperDpapiProtectFailed,
            format!("dpapi protect failed for offline user: {err}"),
        ))
    })?;
    let online_blob = dpapi_protect(online_pwd.as_bytes()).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperDpapiProtectFailed,
            format!("dpapi protect failed for online user: {err}"),
        ))
    })?;
    let users = SandboxUsersFile {
        version: SETUP_VERSION,
        offline: SandboxUserRecord {
            username: offline_user.to_string(),
            password: BASE64.encode(offline_blob),
        },
        online: SandboxUserRecord {
            username: online_user.to_string(),
            password: BASE64.encode(online_blob),
        },
    };
    let users_path = secrets_dir.join("sandbox_users.json");
    let users_json = serde_json::to_vec_pretty(&users).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperUsersFileWriteFailed,
            format!("serialize sandbox users failed: {err}"),
        ))
    })?;
    std::fs::write(&users_path, users_json).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperUsersFileWriteFailed,
            format!(
                "write sandbox users file {} failed: {err}",
                users_path.display()
            ),
        ))
    })?;
    Ok(())
}

pub fn commit_setup_marker(
    zagens_home: &Path,
    offline_user: &str,
    online_user: &str,
    proxy_ports: &[u16],
    allow_local_binding: bool,
) -> Result<()> {
    let marker = SetupMarker {
        version: SETUP_VERSION,
        offline_username: offline_user.to_string(),
        online_username: online_user.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        proxy_ports: proxy_ports.to_vec(),
        allow_local_binding,
    };
    let marker_path = sandbox_dir(zagens_home).join("setup_marker.json");
    let marker_json = serde_json::to_vec_pretty(&marker).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSetupMarkerWriteFailed,
            format!("serialize setup marker failed: {err}"),
        ))
    })?;
    std::fs::write(&marker_path, marker_json).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSetupMarkerWriteFailed,
            format!(
                "write setup marker file {} failed: {err}",
                marker_path.display()
            ),
        ))
    })?;
    Ok(())
}

fn random_password() -> String {
    const CHARS: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()-_=+";
    let mut rng = SmallRng::from_entropy();
    let mut buf = [0u8; 24];
    rng.fill_bytes(&mut buf);
    buf.iter()
        .map(|b| {
            let idx = (*b as usize) % CHARS.len();
            CHARS[idx] as char
        })
        .collect()
}

fn log_line(log: &mut dyn Write, msg: &str) -> Result<()> {
    let ts = chrono::Utc::now().to_rfc3339();
    writeln!(log, "[{ts}] {msg}")?;
    Ok(())
}

#[allow(dead_code)]
fn resolve_sid(name: &str) -> Result<Vec<u8>> {
    let name_w = to_wide(OsStr::new(name));
    let mut sid_buffer = vec![0u8; 68];
    let mut sid_len: u32 = sid_buffer.len() as u32;
    let mut domain: Vec<u16> = Vec::new();
    let mut domain_len: u32 = 0;
    let mut use_type: SID_NAME_USE = 0;
    loop {
        let ok = unsafe {
            LookupAccountNameW(
                std::ptr::null(),
                name_w.as_ptr(),
                sid_buffer.as_mut_ptr() as *mut c_void,
                &mut sid_len,
                domain.as_mut_ptr(),
                &mut domain_len,
                &mut use_type,
            )
        };
        if ok != 0 {
            sid_buffer.truncate(sid_len as usize);
            return Ok(sid_buffer);
        }
        let err = unsafe { GetLastError() };
        if err == ERROR_INSUFFICIENT_BUFFER {
            sid_buffer.resize(sid_len as usize, 0);
            domain.resize(domain_len as usize, 0);
            continue;
        }
        return Err(anyhow::anyhow!(
            "LookupAccountNameW failed for {name}: {err}"
        ));
    }
}

fn sid_bytes_from_string(sid_str: &str) -> Result<Vec<u8>> {
    let sid_w = to_wide(OsStr::new(sid_str));
    let mut psid: *mut c_void = std::ptr::null_mut();
    if unsafe { ConvertStringSidToSidW(sid_w.as_ptr(), &mut psid) } == 0 {
        return Err(anyhow::anyhow!(
            "ConvertStringSidToSidW failed for {sid_str}: {}",
            unsafe { GetLastError() }
        ));
    }
    let sid_len = unsafe { GetLengthSid(psid) };
    let mut out = vec![0u8; sid_len as usize];
    let ok = unsafe { CopySid(sid_len, out.as_mut_ptr() as *mut c_void, psid) };
    unsafe {
        windows_sys::Win32::Foundation::LocalFree(psid as _);
    }
    if ok == 0 {
        return Err(anyhow::anyhow!("CopySid failed for {sid_str}"));
    }
    Ok(out)
}
