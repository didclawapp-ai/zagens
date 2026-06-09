//! Restricted token construction (Codex-aligned flags + SID order).

use std::ffi::c_void;

use anyhow::{Result, anyhow};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_SUCCESS, GetLastError, HANDLE, HLOCAL, LUID, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GRANT_ACCESS, SetEntriesInAclW, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN,
    TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACL, AdjustTokenPrivileges, CopySid, CreateRestrictedToken, CreateWellKnownSid, GetLengthSid,
    GetTokenInformation, LookupPrivilegeValueW, SID_AND_ATTRIBUTES, SetTokenInformation,
    TOKEN_ADJUST_DEFAULT, TOKEN_ADJUST_PRIVILEGES, TOKEN_ADJUST_SESSIONID, TOKEN_ASSIGN_PRIMARY,
    TOKEN_DUPLICATE, TOKEN_PRIVILEGES, TOKEN_QUERY, TokenDefaultDacl, TokenGroups,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::winutil::to_wide;

const DISABLE_MAX_PRIVILEGE: u32 = 0x01;
const LUA_TOKEN: u32 = 0x04;
const WRITE_RESTRICTED: u32 = 0x08;
const GENERIC_ALL: u32 = 0x1000_0000;
const WIN_WORLD_SID: i32 = 1;
const SE_GROUP_LOGON_ID: u32 = 0xC0000000;

#[repr(C)]
struct TokenDefaultDaclInfo {
    default_dacl: *mut ACL,
}

pub struct LocalSid {
    psid: *mut c_void,
}

impl LocalSid {
    pub fn from_string(sid: &str) -> Result<Self> {
        let psid = unsafe { convert_string_sid_to_sid(sid) }
            .ok_or_else(|| anyhow!("invalid SID string: {sid}"))?;
        Ok(Self { psid })
    }

    pub fn as_ptr(&self) -> *mut c_void {
        self.psid
    }
}

impl Drop for LocalSid {
    fn drop(&mut self) {
        if !self.psid.is_null() {
            unsafe {
                LocalFree(self.psid as HLOCAL);
            }
        }
    }
}

unsafe fn convert_string_sid_to_sid(s: &str) -> Option<*mut c_void> {
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn ConvertStringSidToSidW(StringSid: *const u16, Sid: *mut *mut c_void) -> i32;
    }
    let mut psid: *mut c_void = std::ptr::null_mut();
    let ok = unsafe { ConvertStringSidToSidW(to_wide(s).as_ptr(), &mut psid) };
    if ok != 0 { Some(psid) } else { None }
}

unsafe fn world_sid() -> Result<Vec<u8>> {
    let mut size: u32 = 0;
    CreateWellKnownSid(
        WIN_WORLD_SID,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        &mut size,
    );
    let mut buf = vec![0u8; size as usize];
    if CreateWellKnownSid(
        WIN_WORLD_SID,
        std::ptr::null_mut(),
        buf.as_mut_ptr() as *mut c_void,
        &mut size,
    ) == 0
    {
        return Err(anyhow!("CreateWellKnownSid failed: {}", GetLastError()));
    }
    Ok(buf)
}

unsafe fn get_logon_sid_bytes(h_token: HANDLE) -> Result<Vec<u8>> {
    let mut needed: u32 = 0;
    GetTokenInformation(h_token, TokenGroups, std::ptr::null_mut(), 0, &mut needed);
    if needed == 0 {
        return Err(anyhow!("GetTokenInformation(TokenGroups) size failed"));
    }
    let mut buf = vec![0u8; needed as usize];
    if GetTokenInformation(
        h_token,
        TokenGroups,
        buf.as_mut_ptr() as *mut c_void,
        needed,
        &mut needed,
    ) == 0
    {
        return Err(anyhow!("GetTokenInformation(TokenGroups) failed"));
    }
    let group_count = std::ptr::read_unaligned(buf.as_ptr() as *const u32) as usize;
    let after_count = buf.as_ptr().add(std::mem::size_of::<u32>()) as usize;
    let align = std::mem::align_of::<SID_AND_ATTRIBUTES>();
    let aligned = (after_count + (align - 1)) & !(align - 1);
    let groups_ptr = aligned as *const SID_AND_ATTRIBUTES;
    for i in 0..group_count {
        let entry = &*groups_ptr.add(i);
        if (entry.Attributes & SE_GROUP_LOGON_ID) != 0 {
            let len = GetLengthSid(entry.Sid) as usize;
            let mut out = vec![0u8; len];
            CopySid(len as u32, out.as_mut_ptr() as *mut c_void, entry.Sid);
            return Ok(out);
        }
    }
    Err(anyhow!("logon SID not found in token groups"))
}

unsafe fn set_default_dacl(h_token: HANDLE, sids: &[*mut c_void]) -> Result<()> {
    if sids.is_empty() {
        return Ok(());
    }
    let entries: Vec<EXPLICIT_ACCESS_W> = sids
        .iter()
        .map(|sid| EXPLICIT_ACCESS_W {
            grfAccessPermissions: GENERIC_ALL,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: 0,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: 0,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: *sid as *mut u16,
            },
        })
        .collect();
    let mut p_new_dacl: *mut ACL = std::ptr::null_mut();
    if SetEntriesInAclW(
        entries.len() as u32,
        entries.as_ptr(),
        std::ptr::null_mut(),
        &mut p_new_dacl,
    ) != ERROR_SUCCESS
    {
        return Err(anyhow!("SetEntriesInAclW failed"));
    }
    let mut info = TokenDefaultDaclInfo {
        default_dacl: p_new_dacl,
    };
    if SetTokenInformation(
        h_token,
        TokenDefaultDacl,
        &mut info as *mut _ as *mut c_void,
        std::mem::size_of::<TokenDefaultDaclInfo>() as u32,
    ) == 0
    {
        if !p_new_dacl.is_null() {
            LocalFree(p_new_dacl as HLOCAL);
        }
        return Err(anyhow!(
            "SetTokenInformation(TokenDefaultDacl) failed: {}",
            GetLastError()
        ));
    }
    if !p_new_dacl.is_null() {
        LocalFree(p_new_dacl as HLOCAL);
    }
    Ok(())
}

unsafe fn enable_single_privilege(h_token: HANDLE, name: &str) -> Result<()> {
    let mut luid: LUID = std::mem::zeroed();
    if LookupPrivilegeValueW(std::ptr::null(), to_wide(name).as_ptr(), &mut luid) == 0 {
        return Ok(());
    }
    let mut tp: TOKEN_PRIVILEGES = std::mem::zeroed();
    tp.PrivilegeCount = 1;
    tp.Privileges[0].Luid = luid;
    tp.Privileges[0].Attributes = 0x2;
    if AdjustTokenPrivileges(
        h_token,
        0,
        &raw const tp,
        0,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    ) == 0
    {
        return Err(anyhow!("AdjustTokenPrivileges failed: {}", GetLastError()));
    }
    Ok(())
}

unsafe fn get_current_process_token() -> Result<HANDLE> {
    let desired = TOKEN_DUPLICATE
        | TOKEN_QUERY
        | TOKEN_ASSIGN_PRIMARY
        | TOKEN_ADJUST_DEFAULT
        | TOKEN_ADJUST_SESSIONID
        | TOKEN_ADJUST_PRIVILEGES;
    let mut h: HANDLE = 0;
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(
            ProcessHandle: HANDLE,
            DesiredAccess: u32,
            TokenHandle: *mut HANDLE,
        ) -> i32;
    }
    if unsafe { OpenProcessToken(GetCurrentProcess(), desired, &mut h) } == 0 {
        return Err(anyhow!("OpenProcessToken failed: {}", GetLastError()));
    }
    Ok(h)
}

pub struct OwnedToken {
    handle: HANDLE,
}

impl OwnedToken {
    pub fn handle(&self) -> HANDLE {
        self.handle
    }
}

impl Drop for OwnedToken {
    fn drop(&mut self) {
        if self.handle != 0 {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

pub fn create_restricted_token_with_capabilities(cap_sid_strings: &[&str]) -> Result<OwnedToken> {
    let cap_sids: Vec<LocalSid> = cap_sid_strings
        .iter()
        .map(|s| LocalSid::from_string(s))
        .collect::<Result<_>>()?;
    let cap_ptrs: Vec<*mut c_void> = cap_sids.iter().map(|s| s.as_ptr()).collect();

    unsafe {
        let base = get_current_process_token()?;
        let result = create_token_with_caps_from(base, &cap_ptrs, &[]);
        CloseHandle(base);
        result.map(|h| OwnedToken { handle: h })
    }
}

unsafe fn create_token_with_caps_from(
    base_token: HANDLE,
    psid_capabilities: &[*mut c_void],
    extra_restricting_sids: &[*mut c_void],
) -> Result<HANDLE> {
    if psid_capabilities.is_empty() {
        return Err(anyhow!("no capability SIDs provided"));
    }
    let mut logon_sid_bytes = get_logon_sid_bytes(base_token)?;
    let psid_logon = logon_sid_bytes.as_mut_ptr() as *mut c_void;
    let mut everyone = world_sid()?;
    let psid_everyone = everyone.as_mut_ptr() as *mut c_void;

    let mut entries: Vec<SID_AND_ATTRIBUTES> =
        vec![std::mem::zeroed(); psid_capabilities.len() + extra_restricting_sids.len() + 2];
    for (i, psid) in psid_capabilities.iter().enumerate() {
        entries[i].Sid = *psid;
    }
    let extras_idx = psid_capabilities.len();
    for (i, psid) in extra_restricting_sids.iter().enumerate() {
        entries[extras_idx + i].Sid = *psid;
    }
    let logon_idx = extras_idx + extra_restricting_sids.len();
    entries[logon_idx].Sid = psid_logon;
    entries[logon_idx + 1].Sid = psid_everyone;

    let mut new_token: HANDLE = 0;
    let flags = DISABLE_MAX_PRIVILEGE | LUA_TOKEN | WRITE_RESTRICTED;
    if CreateRestrictedToken(
        base_token,
        flags,
        0,
        std::ptr::null(),
        0,
        std::ptr::null(),
        entries.len() as u32,
        entries.as_mut_ptr(),
        &mut new_token,
    ) == 0
    {
        return Err(anyhow!("CreateRestrictedToken failed: {}", GetLastError()));
    }

    let mut dacl_sids: Vec<*mut c_void> = Vec::with_capacity(psid_capabilities.len() + 2);
    dacl_sids.push(psid_logon);
    dacl_sids.push(psid_everyone);
    dacl_sids.extend_from_slice(psid_capabilities);
    set_default_dacl(new_token, &dacl_sids)?;
    enable_single_privilege(new_token, "SeChangeNotifyPrivilege")?;
    Ok(new_token)
}
