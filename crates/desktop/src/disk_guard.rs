//! Free-space checks for user data (`~/.zagens`) and the active workspace volume.
//! Used to pause in-flight turns before ENOSPC corrupts sessions or billing continues blindly.

use serde::Serialize;
use std::path::Path;
use zagens_config::user_data_root;

/// Below this: pause turns and show critical UI.
pub const CRITICAL_FREE_BYTES: u64 = 100 * 1024 * 1024;
/// Below this: warn only.
pub const WARN_FREE_BYTES: u64 = 500 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoragePressureLevel {
    Ok,
    Warn,
    Critical,
}

#[derive(Debug, Clone, Serialize)]
pub struct VolumePressure {
    pub path: String,
    pub free_bytes: u64,
    pub level: StoragePressureLevel,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoragePressureSnapshot {
    pub pause_turns: bool,
    pub user_data: VolumePressure,
    pub workspace: Option<VolumePressure>,
}

fn level_from_free(free: u64) -> StoragePressureLevel {
    if free < CRITICAL_FREE_BYTES {
        StoragePressureLevel::Critical
    } else if free < WARN_FREE_BYTES {
        StoragePressureLevel::Warn
    } else {
        StoragePressureLevel::Ok
    }
}

fn severity(a: StoragePressureLevel, b: StoragePressureLevel) -> StoragePressureLevel {
    use StoragePressureLevel::*;
    match (a, b) {
        (Critical, _) | (_, Critical) => Critical,
        (Warn, _) | (_, Warn) => Warn,
        _ => Ok,
    }
}

/// Returns available bytes for the volume containing `path`.
pub fn volume_free_bytes(path: &Path) -> Result<u64, String> {
    #[cfg(windows)]
    {
        windows_free_bytes(path)
    }
    #[cfg(unix)]
    {
        unix_free_bytes(path)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        Ok(u64::MAX)
    }
}

#[cfg(windows)]
fn windows_free_bytes(path: &Path) -> Result<u64, String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            lp_directory_name: *const u16,
            lp_free_bytes_available_to_caller: *mut u64,
            lp_total_number_of_bytes: *mut u64,
            lp_total_number_of_free_bytes: *mut u64,
        ) -> i32;
    }

    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let s = canonical.to_string_lossy();
    let root = if s.starts_with(r"\\") {
        s.to_string()
    } else if s.len() >= 2 && s.as_bytes()[1] == b':' {
        format!("{}\\", &s[..2])
    } else {
        s.to_string()
    };
    let wide: Vec<u16> = OsStr::new(&root).encode_wide().chain(Some(0)).collect();
    let mut free_avail = 0u64;
    let mut total = 0u64;
    let mut total_free = 0u64;
    let ok =
        unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free_avail, &mut total, &mut total_free) };
    if ok == 0 {
        return Err(format!("无法读取磁盘剩余空间（{}）", canonical.display()));
    }
    Ok(free_avail)
}

#[cfg(unix)]
fn unix_free_bytes(path: &Path) -> Result<u64, String> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;

    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let c_path = CString::new(canonical.to_string_lossy().as_bytes())
        .map_err(|_| "路径包含空字节".to_string())?;
    let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if rc != 0 {
        return Err(format!("无法读取磁盘剩余空间（{}）", canonical.display()));
    }
    let stat = unsafe { stat.assume_init() };
    // Field width varies (u64 on Linux, narrower on macOS). Widen via u128 so we
    // avoid both `as u64` (unnecessary_cast on Linux) and `u64::from` (useless_conversion).
    let free = (stat.f_bavail as u128 * stat.f_frsize as u128) as u64;
    Ok(free)
}

pub fn storage_pressure_snapshot(
    workspace_root: Option<&str>,
) -> Result<StoragePressureSnapshot, String> {
    let ud_path = user_data_root().map_err(|e| e.to_string())?;
    let ud_free = volume_free_bytes(&ud_path)?;
    let ud_level = level_from_free(ud_free);
    let user_data = VolumePressure {
        path: ud_path.display().to_string(),
        free_bytes: ud_free,
        level: ud_level,
    };

    let workspace = workspace_root.and_then(|w| {
        let trimmed = w.trim();
        if trimmed.is_empty() {
            return None;
        }
        let p = Path::new(trimmed);
        let free = volume_free_bytes(p).ok()?;
        Some(VolumePressure {
            path: trimmed.to_string(),
            free_bytes: free,
            level: level_from_free(free),
        })
    });

    let combined = workspace
        .as_ref()
        .map(|w| severity(user_data.level, w.level))
        .unwrap_or(user_data.level);

    Ok(StoragePressureSnapshot {
        pause_turns: combined == StoragePressureLevel::Critical,
        user_data,
        workspace,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_thresholds() {
        assert_eq!(level_from_free(0), StoragePressureLevel::Critical);
        assert_eq!(
            level_from_free(CRITICAL_FREE_BYTES - 1),
            StoragePressureLevel::Critical
        );
        assert_eq!(
            level_from_free(CRITICAL_FREE_BYTES),
            StoragePressureLevel::Warn
        );
        assert_eq!(level_from_free(WARN_FREE_BYTES), StoragePressureLevel::Ok);
    }
}
