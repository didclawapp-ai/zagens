//! Optional isolated Win32 desktop for sandbox child processes (PR-3.2).

use std::sync::Mutex;

use anyhow::{Result, anyhow};
use once_cell::sync::OnceCell;
use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
use windows_sys::Win32::System::StationsAndDesktops::{
    CloseDesktop, CreateDesktopW, DESKTOP_CREATEWINDOW, DESKTOP_ENUMERATE, DESKTOP_HOOKCONTROL,
    DESKTOP_JOURNALPLAYBACK, DESKTOP_JOURNALRECORD, DESKTOP_READOBJECTS, DESKTOP_SWITCHDESKTOP,
    DESKTOP_WRITEOBJECTS,
};

use crate::winutil::to_wide;

const DESKTOP_CREATEMENU: u32 = 0x0004;

static SESSION_DESKTOP: OnceCell<Mutex<PrivateDesktop>> = OnceCell::new();

struct PrivateDesktop {
    handle: HANDLE,
    wide_name: Vec<u16>,
}

impl Drop for PrivateDesktop {
    fn drop(&mut self) {
        if self.handle != 0 {
            unsafe {
                CloseDesktop(self.handle);
            }
            self.handle = 0;
        }
    }
}

fn desktop_access_mask() -> u32 {
    DESKTOP_READOBJECTS
        | DESKTOP_CREATEWINDOW
        | DESKTOP_CREATEMENU
        | DESKTOP_HOOKCONTROL
        | DESKTOP_JOURNALRECORD
        | DESKTOP_JOURNALPLAYBACK
        | DESKTOP_ENUMERATE
        | DESKTOP_WRITEOBJECTS
        | DESKTOP_SWITCHDESKTOP
}

fn ensure_private_desktop() -> Result<&'static Mutex<PrivateDesktop>> {
    SESSION_DESKTOP.get_or_try_init(|| {
        let name = format!("ZagensSbx-{}", std::process::id());
        let wide = to_wide(&name);
        let handle = unsafe {
            CreateDesktopW(
                wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
                desktop_access_mask(),
                std::ptr::null_mut(),
            )
        };
        if handle == 0 {
            return Err(anyhow!("CreateDesktopW({name}) failed: {}", unsafe {
                GetLastError()
            }));
        }
        let wide_name = to_wide(&format!(r"Winsta0\{name}"));
        Ok(Mutex::new(PrivateDesktop { handle, wide_name }))
    })
}

/// Returns the wide desktop string for `STARTUPINFOW.lpDesktop`.
///
/// When `private_desktop` is false, returns `Winsta0\\Default`.
pub fn desktop_wide_name(private_desktop: bool) -> Result<Vec<u16>> {
    if private_desktop {
        let guard = ensure_private_desktop()?
            .lock()
            .map_err(|_| anyhow!("private desktop lock poisoned"))?;
        Ok(guard.wide_name.clone())
    } else {
        Ok(to_wide(r"Winsta0\Default"))
    }
}
