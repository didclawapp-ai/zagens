//! Redirect process stderr during TUI so engine `eprintln!` probes do not corrupt ratatui.

use std::fs::{File, OpenOptions};
use std::path::PathBuf;

use anyhow::{Context, Result};

/// Keeps stderr redirected for the lifetime of the TUI session.
pub struct StderrLogGuard {
    _file: File,
    #[cfg(unix)]
    previous_fd: libc::c_int,
    #[cfg(windows)]
    previous: windows::Win32::Foundation::HANDLE,
}

impl StderrLogGuard {
    pub fn install() -> Result<Self> {
        // SAFETY: set before other threads; TUI is single-threaded for terminal I/O.
        unsafe { std::env::set_var("ZAGENS_TUI", "1") };
        let path = log_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("open stderr log {}", path.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let previous_fd = unsafe { libc::dup(2) };
            if previous_fd < 0 {
                return Err(std::io::Error::last_os_error()).context("dup stderr");
            }
            let rc = unsafe { libc::dup2(file.as_raw_fd(), 2) };
            if rc < 0 {
                unsafe { libc::close(previous_fd) };
                return Err(std::io::Error::last_os_error()).context("dup2 stderr");
            }
            Ok(Self {
                _file: file,
                previous_fd,
            })
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows::Win32::Foundation::HANDLE;
            use windows::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, SetStdHandle};

            let previous = unsafe { GetStdHandle(STD_ERROR_HANDLE) }
                .context("GetStdHandle(STD_ERROR_HANDLE)")?;
            unsafe {
                SetStdHandle(STD_ERROR_HANDLE, HANDLE(file.as_raw_handle() as _))
                    .context("SetStdHandle(STD_ERROR_HANDLE)")?;
            }
            Ok(Self {
                _file: file,
                previous,
            })
        }
    }
}

impl Drop for StderrLogGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::dup2(self.previous_fd, 2);
            libc::close(self.previous_fd);
        }
        #[cfg(windows)]
        {
            use windows::Win32::System::Console::{STD_ERROR_HANDLE, SetStdHandle};
            unsafe {
                let _ = SetStdHandle(STD_ERROR_HANDLE, self.previous);
            }
        }
    }
}

fn log_path() -> Result<PathBuf> {
    zagens_config::user_data_path("logs/tui-probe.log").map_err(|e| anyhow::anyhow!("{e}"))
}
