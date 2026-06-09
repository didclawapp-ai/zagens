use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

pub fn to_wide<S: AsRef<OsStr>>(s: S) -> Vec<u16> {
    let mut v: Vec<u16> = s.as_ref().encode_wide().collect();
    v.push(0);
    v
}

pub fn format_last_error(code: i32) -> String {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::System::Diagnostics::Debug::{
        FORMAT_MESSAGE_ALLOCATE_BUFFER, FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS,
        FormatMessageW,
    };

    let mut buf: *mut u16 = std::ptr::null_mut();
    let len = unsafe {
        FormatMessageW(
            FORMAT_MESSAGE_ALLOCATE_BUFFER
                | FORMAT_MESSAGE_FROM_SYSTEM
                | FORMAT_MESSAGE_IGNORE_INSERTS,
            std::ptr::null(),
            code as u32,
            0,
            &mut buf as *mut *mut u16 as *mut u16,
            0,
            std::ptr::null_mut(),
        )
    };
    if len == 0 || buf.is_null() {
        return format!("Win32 error {code}");
    }
    let slice = unsafe { std::slice::from_raw_parts(buf, len as usize) };
    let msg = String::from_utf16_lossy(slice).trim().to_string();
    unsafe {
        LocalFree(buf as _);
    }
    msg
}
