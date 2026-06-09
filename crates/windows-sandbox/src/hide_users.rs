//! Hide sandbox local users from the Windows welcome screen (Winlogon UserList).

use std::ffi::OsStr;
use std::path::Path;

use crate::logging::log_note;
use crate::winutil::to_wide;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, GetLastError};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE,
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
};

const USERLIST_KEY_PATH: &str =
    r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon\SpecialAccounts\UserList";

pub fn hide_newly_created_users(usernames: &[String], log_base: &Path) {
    if usernames.is_empty() {
        return;
    }
    if let Err(err) = hide_users_in_winlogon(usernames) {
        log_note(
            &format!("hide users: failed to update Winlogon UserList: {err}"),
            Some(log_base),
        );
    }
}

/// Removes the Winlogon `UserList` hide entries for the sandbox users
/// (teardown step 2, design §8.5). Missing values/keys are tolerated.
pub fn unhide_removed_users(usernames: &[String], log_base: &Path) {
    if usernames.is_empty() {
        return;
    }
    if let Err(err) = unhide_users_in_winlogon(usernames) {
        log_note(
            &format!("unhide users: failed to clean Winlogon UserList: {err}"),
            Some(log_base),
        );
    }
}

fn unhide_users_in_winlogon(usernames: &[String]) -> anyhow::Result<()> {
    let path_w = to_wide(USERLIST_KEY_PATH);
    let mut key: HKEY = 0;
    let open = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            path_w.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut key,
        )
    };
    if open as u32 == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    if open != 0 {
        return Err(anyhow::anyhow!("RegOpenKeyExW failed for UserList: {open}"));
    }
    let mut first_error = None;
    for username in usernames {
        let name_w = to_wide(OsStr::new(username));
        let code = unsafe { RegDeleteValueW(key, name_w.as_ptr()) };
        if code != 0 && code as u32 != ERROR_FILE_NOT_FOUND && first_error.is_none() {
            first_error = Some(anyhow::anyhow!(
                "RegDeleteValueW failed for {username}: {code}"
            ));
        }
    }
    unsafe {
        RegCloseKey(key);
    }
    match first_error {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn hide_users_in_winlogon(usernames: &[String]) -> anyhow::Result<()> {
    let key = create_userlist_key()?;
    for username in usernames {
        let name_w = to_wide(OsStr::new(username));
        let value: u32 = 0;
        let ok = unsafe {
            RegSetValueExW(
                key,
                name_w.as_ptr(),
                0,
                REG_DWORD,
                &value as *const _ as *const u8,
                std::mem::size_of::<u32>() as u32,
            )
        };
        if ok != 0 {
            unsafe {
                RegCloseKey(key);
            }
            return Err(anyhow::anyhow!(
                "RegSetValueExW failed for {username}: {}",
                unsafe { GetLastError() }
            ));
        }
    }
    unsafe {
        RegCloseKey(key);
    }
    Ok(())
}

fn create_userlist_key() -> anyhow::Result<HKEY> {
    let path_w = to_wide(USERLIST_KEY_PATH);
    let mut key: HKEY = 0;
    let mut disposition: u32 = 0;
    let ok = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            path_w.as_ptr(),
            0,
            std::ptr::null_mut(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut key,
            &mut disposition,
        )
    };
    if ok != 0 {
        return Err(anyhow::anyhow!(
            "RegCreateKeyExW failed for UserList: {}",
            unsafe { GetLastError() }
        ));
    }
    Ok(key)
}
