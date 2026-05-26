//! Binary entry helpers (B3.1).

#[cfg(windows)]
pub fn configure_windows_console_utf8() {
    use windows::Win32::System::Console::{SetConsoleCP, SetConsoleOutputCP};

    const CP_UTF8: u32 = 65001;
    unsafe {
        let _ = SetConsoleCP(CP_UTF8);
        let _ = SetConsoleOutputCP(CP_UTF8);
    }
}

#[cfg(not(windows))]
pub fn configure_windows_console_utf8() {}
