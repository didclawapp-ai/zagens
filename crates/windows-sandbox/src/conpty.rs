//! ConPTY (`CreatePseudoConsole`) spawn for restricted-token children (PR-3.1).

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::path::Path;
use std::ptr;

use anyhow::{Result, anyhow};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
    SetHandleInformation,
};
use windows_sys::Win32::System::Console::{
    COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
    STARTUPINFOW, TerminateProcess, UpdateProcThreadAttribute,
};

use crate::private_desktop::desktop_wide_name;
use crate::process::{ManagedProcess, SpawnDenial, argv_to_command_line, make_env_block};
use crate::winutil::to_wide;

fn coord_dim(value: u16) -> i16 {
    i16::try_from(value).unwrap_or(i16::MAX)
}

const PSEUDOCONSOLE_RESIZE_QUIRK: u32 = 0x2;
const PSEUDOCONSOLE_WIN32_INPUT_MODE: u32 = 0x4;

/// Default ConPTY geometry when the parent does not specify a size.
pub const DEFAULT_CONPTY_ROWS: u16 = 24;
pub const DEFAULT_CONPTY_COLS: u16 = 80;

struct ConPtyAttributeList {
    siex: STARTUPINFOEXW,
    attr_storage: Vec<u8>,
    hpc: HPCON,
}

impl ConPtyAttributeList {
    fn new(hpc: HPCON, desktop: *mut u16) -> Result<Self> {
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
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                hpc as *const c_void,
                size_of::<HPCON>(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if ok == 0 {
            unsafe {
                DeleteProcThreadAttributeList(attr_list);
            }
            return Err(anyhow!(
                "UpdateProcThreadAttribute(PSEUDOCONSOLE) failed: {}",
                unsafe { GetLastError() }
            ));
        }

        let mut si: STARTUPINFOW = unsafe { zeroed() };
        si.cb = size_of::<STARTUPINFOEXW>() as u32;
        si.dwFlags = STARTF_USESTDHANDLES;
        si.hStdInput = INVALID_HANDLE_VALUE;
        si.hStdOutput = INVALID_HANDLE_VALUE;
        si.hStdError = INVALID_HANDLE_VALUE;
        si.lpDesktop = desktop;

        Ok(Self {
            siex: STARTUPINFOEXW {
                StartupInfo: si,
                lpAttributeList: attr_list,
            },
            attr_storage,
            hpc,
        })
    }

    fn startup_info_ptr(&self) -> *const STARTUPINFOW {
        ptr::addr_of!(self.siex).cast()
    }

    fn creation_flags(&self) -> u32 {
        EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT
    }
    fn disarm(&mut self) {
        self.hpc = 0;
    }
}

impl Drop for ConPtyAttributeList {
    fn drop(&mut self) {
        if !self.siex.lpAttributeList.is_null() {
            unsafe {
                DeleteProcThreadAttributeList(self.siex.lpAttributeList);
            }
        }
        if self.hpc != 0 && self.hpc != INVALID_HANDLE_VALUE {
            unsafe {
                ClosePseudoConsole(self.hpc);
            }
        }
    }
}

/// Spawn a restricted-token child attached to a ConPTY session.
pub fn spawn_with_conpty(
    token: HANDLE,
    argv: &[String],
    cwd: &Path,
    env: &HashMap<String, String>,
    rows: u16,
    cols: u16,
    private_desktop: bool,
) -> Result<(ManagedProcess, HPCON)> {
    unsafe {
        let mut input_read: HANDLE = 0;
        let mut input_write: HANDLE = 0;
        let mut output_read: HANDLE = 0;
        let mut output_write: HANDLE = 0;

        if CreatePipe(&mut input_read, &mut input_write, ptr::null_mut(), 0) == 0 {
            return Err(anyhow!(
                "CreatePipe(conpty input) failed: {}",
                GetLastError()
            ));
        }
        if CreatePipe(&mut output_read, &mut output_write, ptr::null_mut(), 0) == 0 {
            CloseHandle(input_read);
            CloseHandle(input_write);
            return Err(anyhow!(
                "CreatePipe(conpty output) failed: {}",
                GetLastError()
            ));
        }

        for h in [input_read, output_write] {
            SetHandleInformation(h, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
        }

        let size = COORD {
            X: coord_dim(cols),
            Y: coord_dim(rows),
        };
        let mut hpc: HPCON = 0;
        let hr = CreatePseudoConsole(
            size,
            input_read,
            output_write,
            PSEUDOCONSOLE_RESIZE_QUIRK | PSEUDOCONSOLE_WIN32_INPUT_MODE,
            &mut hpc,
        );
        CloseHandle(input_read);
        CloseHandle(output_write);
        if hr != 0 {
            CloseHandle(input_write);
            CloseHandle(output_read);
            return Err(anyhow!("CreatePseudoConsole failed: HRESULT {hr}"));
        }

        let desktop = desktop_wide_name(private_desktop)?;
        let mut startup = ConPtyAttributeList::new(hpc, desktop.as_ptr() as *mut u16)?;

        let cmdline_str = argv_to_command_line(argv);
        let mut cmdline = to_wide(&cmdline_str);
        let env_block = if env.is_empty() {
            None
        } else {
            Some(make_env_block(env))
        };
        let cwd_wide = to_wide(cwd);
        let env_ptr = env_block
            .as_ref()
            .map_or(ptr::null_mut(), |block| block.as_ptr() as *mut c_void);

        let mut pi: PROCESS_INFORMATION = zeroed();
        let ok = CreateProcessAsUserW(
            token,
            ptr::null(),
            cmdline.as_mut_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            startup.creation_flags(),
            env_ptr,
            cwd_wide.as_ptr(),
            startup.startup_info_ptr(),
            &mut pi,
        );

        if ok == 0 {
            let err = GetLastError();
            CloseHandle(input_write);
            CloseHandle(output_read);
            return Err(anyhow::Error::new(SpawnDenial {
                win32_code: err,
                api: "CreateProcessAsUserW(conpty)",
            }));
        }

        let job = create_kill_on_close_job()?;
        if AssignProcessToJobObject(job, pi.hProcess) == 0 {
            let err = GetLastError();
            TerminateProcess(pi.hProcess, 1);
            CloseHandle(pi.hProcess);
            CloseHandle(pi.hThread);
            CloseHandle(job);
            CloseHandle(input_write);
            CloseHandle(output_read);
            return Err(anyhow!(
                "AssignProcessToJobObject failed: {err}: {}",
                format_last_error(err as i32)
            ));
        }

        CloseHandle(pi.hThread);

        startup.disarm();

        Ok((
            ManagedProcess::from_spawn_handles(pi.hProcess, job, output_read, 0, input_write, hpc),
            hpc,
        ))
    }
}

/// Resize an active pseudo console (IPC `Resize` handler in the command runner).
pub fn resize_conpty(hpc: HPCON, rows: u16, cols: u16) -> Result<()> {
    let size = COORD {
        X: coord_dim(cols),
        Y: coord_dim(rows),
    };
    let hr = unsafe { ResizePseudoConsole(hpc, size) };
    if hr != 0 {
        return Err(anyhow!("ResizePseudoConsole failed: HRESULT {hr}"));
    }
    Ok(())
}

unsafe fn create_kill_on_close_job() -> Result<HANDLE> {
    let job = CreateJobObjectW(ptr::null(), ptr::null());
    if job == 0 {
        return Err(anyhow!("CreateJobObjectW failed: {}", GetLastError()));
    }
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if SetInformationJobObject(
        job,
        JobObjectExtendedLimitInformation,
        &mut info as *mut _ as *mut c_void,
        size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
    ) == 0
    {
        CloseHandle(job);
        return Err(anyhow!(
            "SetInformationJobObject failed: {}",
            GetLastError()
        ));
    }
    Ok(job)
}

fn format_last_error(code: i32) -> String {
    crate::winutil::format_last_error(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_geometry_is_reasonable() {
        assert_eq!(DEFAULT_CONPTY_ROWS, 24);
        assert_eq!(DEFAULT_CONPTY_COLS, 80);
    }
}
