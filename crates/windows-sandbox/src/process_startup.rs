//! Extended startup info with PROC_THREAD_ATTRIBUTE_HANDLE_LIST for restricted-token
//! child spawns (design §9.6 — pipe handles must be explicitly inherited).

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr;

use anyhow::{Result, anyhow};
use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
use windows_sys::Win32::System::Threading::{
    DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, InitializeProcThreadAttributeList,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, STARTF_USESTDHANDLES,
    STARTUPINFOEXW, STARTUPINFOW, UpdateProcThreadAttribute,
};

/// Owns a `STARTUPINFOEXW` whose handle list whitelists stdio pipe ends for
/// `CreateProcessAsUserW` under a restricted token.
pub struct StartupWithHandleList {
    pub siex: STARTUPINFOEXW,
    /// Keeps the attribute list buffer alive for `lpAttributeList`.
    _attr_storage: Vec<u8>,
    /// Keeps the handle array alive for the attribute list pointer.
    _handles: Vec<HANDLE>,
}

impl StartupWithHandleList {
    pub fn new(
        std_input: HANDLE,
        std_output: HANDLE,
        std_error: HANDLE,
        desktop: *mut u16,
    ) -> Result<Self> {
        let handles: Vec<HANDLE> = [std_input, std_output, std_error]
            .into_iter()
            .filter(|&h| h != 0)
            .collect();

        let attr_count = 1u32;
        let mut attr_size: usize = 0;
        unsafe {
            let _ =
                InitializeProcThreadAttributeList(ptr::null_mut(), attr_count, 0, &mut attr_size);
        }
        if attr_size == 0 {
            return Err(anyhow!(
                "InitializeProcThreadAttributeList size query returned 0"
            ));
        }
        let mut attr_storage = vec![0u8; attr_size];
        let attr_list = attr_storage.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;

        let ok =
            unsafe { InitializeProcThreadAttributeList(attr_list, attr_count, 0, &mut attr_size) };
        if ok == 0 {
            return Err(anyhow!(
                "InitializeProcThreadAttributeList failed: {}",
                unsafe { GetLastError() }
            ));
        }
        let ok = unsafe {
            UpdateProcThreadAttribute(
                attr_list,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr() as *const c_void,
                handles.len() * size_of::<HANDLE>(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if ok == 0 {
            unsafe {
                DeleteProcThreadAttributeList(attr_list);
            }
            return Err(anyhow!(
                "UpdateProcThreadAttribute(HANDLE_LIST) failed: {}",
                unsafe { GetLastError() }
            ));
        }

        let mut si: STARTUPINFOW = unsafe { zeroed() };
        si.cb = size_of::<STARTUPINFOEXW>() as u32;
        si.dwFlags = STARTF_USESTDHANDLES;
        si.hStdInput = std_input;
        si.hStdOutput = std_output;
        si.hStdError = std_error;
        si.lpDesktop = desktop;

        Ok(Self {
            siex: STARTUPINFOEXW {
                StartupInfo: si,
                lpAttributeList: attr_list,
            },
            _attr_storage: attr_storage,
            _handles: handles,
        })
    }

    pub fn creation_flags(&self, base: u32) -> u32 {
        base | EXTENDED_STARTUPINFO_PRESENT
    }

    pub fn startup_info_ptr(&self) -> *const STARTUPINFOW {
        ptr::addr_of!(self.siex).cast()
    }
}

impl Drop for StartupWithHandleList {
    fn drop(&mut self) {
        if !self.siex.lpAttributeList.is_null() {
            unsafe {
                DeleteProcThreadAttributeList(self.siex.lpAttributeList);
            }
        }
    }
}
