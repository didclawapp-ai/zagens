//! NTFS DACL helpers (deny-read ACE). Ported in spirit from Codex `windows-sandbox-rs/acl.rs`.

use std::ffi::c_void;
use std::path::Path;

use anyhow::{Result, anyhow};
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, HLOCAL, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW,
    TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACCESS_DENIED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
    DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
};

use crate::winutil::to_wide;

const ACCESS_DENIED_ACE_TYPE: u8 = 1;
const INHERITED_ACE: u8 = 0x10;
const UNPROTECTED_DACL_SECURITY_INFORMATION: u32 = 0x2000_0000;
const INHERIT_ONLY_ACE: u8 = 0x08;
const GENERIC_READ_MASK: u32 = 0x8000_0000;
const DENY_ACCESS: u32 = 3;
const GRANT_ACCESS: u32 = 1;
const CONTAINER_INHERIT_ACE: u32 = 0x2;
const OBJECT_INHERIT_ACE: u32 = 0x1;
const DELETE: u32 = 0x0001_0000;
const FILE_GENERIC_EXECUTE: u32 = 0x2000_0000;

const WRITE_DENY_MASK: u32 =
    FILE_GENERIC_WRITE | FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES | 0x4000_0000;

const WRITE_GRANT_MASK: u32 =
    FILE_GENERIC_READ | FILE_GENERIC_EXECUTE | FILE_GENERIC_WRITE | DELETE;

unsafe fn dacl_has_read_deny_for_sid(
    p_dacl: *mut ACL,
    psid: *mut c_void,
    require_inherited: bool,
) -> bool {
    if p_dacl.is_null() {
        return false;
    }
    let mut info: ACL_SIZE_INFORMATION = std::mem::zeroed();
    if GetAclInformation(
        p_dacl as *const ACL,
        &mut info as *mut _ as *mut c_void,
        std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
        AclSizeInformation,
    ) == 0
    {
        return false;
    }
    let deny_read_mask = FILE_GENERIC_READ | GENERIC_READ_MASK;
    for i in 0..info.AceCount {
        let mut p_ace: *mut c_void = std::ptr::null_mut();
        if GetAce(p_dacl as *const ACL, i, &mut p_ace) == 0 {
            continue;
        }
        let hdr = &*(p_ace as *const ACE_HEADER);
        if hdr.AceType != ACCESS_DENIED_ACE_TYPE || (hdr.AceFlags & INHERIT_ONLY_ACE) != 0 {
            continue;
        }
        if require_inherited && (hdr.AceFlags & INHERITED_ACE) == 0 {
            continue;
        }
        let ace = &*(p_ace as *const ACCESS_DENIED_ACE);
        let sid_ptr = (p_ace as usize
            + std::mem::size_of::<ACE_HEADER>()
            + std::mem::size_of::<u32>()) as *mut c_void;
        if EqualSid(sid_ptr, psid) != 0 && (ace.Mask & deny_read_mask) != 0 {
            return true;
        }
    }
    false
}

pub fn has_deny_read_ace(path: &Path, psid: *mut c_void) -> bool {
    has_deny_read_ace_inner(path, psid, false)
}

pub fn has_inherited_deny_read_ace(path: &Path, psid: *mut c_void) -> bool {
    has_deny_read_ace_inner(path, psid, true)
}

fn has_deny_read_ace_inner(path: &Path, psid: *mut c_void, require_inherited: bool) -> bool {
    unsafe {
        let mut p_sd: *mut c_void = std::ptr::null_mut();
        let mut p_dacl: *mut ACL = std::ptr::null_mut();
        if GetNamedSecurityInfoW(
            to_wide(path).as_ptr(),
            1,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut p_dacl,
            std::ptr::null_mut(),
            &mut p_sd,
        ) != ERROR_SUCCESS
        {
            if !p_sd.is_null() {
                LocalFree(p_sd as HLOCAL);
            }
            return false;
        }
        let found = dacl_has_read_deny_for_sid(p_dacl, psid, require_inherited);
        if !p_sd.is_null() {
            LocalFree(p_sd as HLOCAL);
        }
        found
    }
}

/// Reset a child object's DACL to inherit from its parent (clears explicit ACEs).
pub fn restore_inherited_dacl(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    unsafe {
        let mut acl_buf = vec![0u8; 4096];
        let p_acl = acl_buf.as_mut_ptr() as *mut ACL;
        #[link(name = "advapi32")]
        unsafe extern "system" {
            fn InitializeAcl(p_acl: *mut ACL, n_acl_length: u32, dw_acl_revision: u32) -> i32;
        }
        if InitializeAcl(p_acl, acl_buf.len() as u32, 2) == 0 {
            return Err(anyhow!("InitializeAcl failed"));
        }
        let code = SetNamedSecurityInfoW(
            to_wide(path).as_ptr() as *mut u16,
            1,
            DACL_SECURITY_INFORMATION | UNPROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            p_acl,
            std::ptr::null_mut(),
        );
        Ok(code == ERROR_SUCCESS)
    }
}

pub fn apply_deny_read_ace(path: &Path, psid: *mut c_void) -> Result<bool> {
    if has_deny_read_ace(path, psid) {
        return Ok(false);
    }
    if !path.exists() {
        std::fs::create_dir_all(path)
            .map_err(|e| anyhow!("create deny-read path {}: {e}", path.display()))?;
    }
    unsafe { add_deny_ace(path, psid, FILE_GENERIC_READ | GENERIC_READ_MASK) }
}

pub fn apply_deny_write_ace(path: &Path, psid: *mut c_void) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    unsafe { add_deny_ace(path, psid, WRITE_DENY_MASK) }
}

pub fn apply_grant_write_ace(path: &Path, psid: *mut c_void) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    unsafe { add_grant_ace(path, psid, WRITE_GRANT_MASK) }
}

unsafe fn add_grant_ace(path: &Path, psid: *mut c_void, mask: u32) -> Result<bool> {
    modify_dacl(path, psid, mask, GRANT_ACCESS)
}

unsafe fn add_deny_ace(path: &Path, psid: *mut c_void, mask: u32) -> Result<bool> {
    modify_dacl(path, psid, mask, DENY_ACCESS)
}

unsafe fn modify_dacl(path: &Path, psid: *mut c_void, mask: u32, mode: u32) -> Result<bool> {
    let mut p_sd: *mut c_void = std::ptr::null_mut();
    let mut p_dacl: *mut ACL = std::ptr::null_mut();
    let code = GetNamedSecurityInfoW(
        to_wide(path).as_ptr(),
        1,
        DACL_SECURITY_INFORMATION,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        &mut p_dacl,
        std::ptr::null_mut(),
        &mut p_sd,
    );
    if code != ERROR_SUCCESS {
        return Err(anyhow!("GetNamedSecurityInfoW failed: {code}"));
    }
    let trustee = TRUSTEE_W {
        pMultipleTrustee: std::ptr::null_mut(),
        MultipleTrusteeOperation: 0,
        TrusteeForm: TRUSTEE_IS_SID,
        TrusteeType: TRUSTEE_IS_UNKNOWN,
        ptstrName: psid as *mut u16,
    };
    let mut explicit: EXPLICIT_ACCESS_W = std::mem::zeroed();
    explicit.grfAccessPermissions = mask;
    explicit.grfAccessMode = mode as i32;
    explicit.grfInheritance = CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE;
    explicit.Trustee = trustee;
    let mut p_new_dacl: *mut ACL = std::ptr::null_mut();
    let mut added = false;
    if SetEntriesInAclW(1, &explicit, p_dacl, &mut p_new_dacl) == ERROR_SUCCESS {
        if SetNamedSecurityInfoW(
            to_wide(path).as_ptr() as *mut u16,
            1,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            p_new_dacl,
            std::ptr::null_mut(),
        ) == ERROR_SUCCESS
        {
            added = true;
        }
        if !p_new_dacl.is_null() {
            LocalFree(p_new_dacl as HLOCAL);
        }
    }
    if !p_sd.is_null() {
        LocalFree(p_sd as HLOCAL);
    }
    Ok(added)
}

pub fn revoke_ace(path: &Path, psid: *mut c_void) {
    unsafe {
        let mut p_sd: *mut c_void = std::ptr::null_mut();
        let mut p_dacl: *mut ACL = std::ptr::null_mut();
        if GetNamedSecurityInfoW(
            to_wide(path).as_ptr(),
            1,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut p_dacl,
            std::ptr::null_mut(),
            &mut p_sd,
        ) != ERROR_SUCCESS
        {
            if !p_sd.is_null() {
                LocalFree(p_sd as HLOCAL);
            }
            return;
        }
        let trustee = TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: 0,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: psid as *mut u16,
        };
        let mut explicit: EXPLICIT_ACCESS_W = std::mem::zeroed();
        explicit.grfAccessMode = 4; // REVOKE_ACCESS
        explicit.grfInheritance = CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE;
        explicit.Trustee = trustee;
        let mut p_new_dacl: *mut ACL = std::ptr::null_mut();
        if SetEntriesInAclW(1, &explicit, p_dacl, &mut p_new_dacl) == ERROR_SUCCESS {
            let _ = SetNamedSecurityInfoW(
                to_wide(path).as_ptr() as *mut u16,
                1,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                p_new_dacl,
                std::ptr::null_mut(),
            );
            if !p_new_dacl.is_null() {
                LocalFree(p_new_dacl as HLOCAL);
            }
        }
        if !p_sd.is_null() {
            LocalFree(p_sd as HLOCAL);
        }
    }
}
