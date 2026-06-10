//! Everyone-writable audit scan (PR-2.7, design §13.6).
//!
//! Write-restricted tokens include **Everyone** in the restricting list, so a
//! directory whose DACL allows Everyone to write stays writable from inside
//! the sandbox — an inherent trade-off of the scheme. This scan finds such
//! directories (time-boxed, bounded per directory) so setup can warn and
//! callers can optionally pin a cap-SID deny-write ACE on the offenders.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::acl::{apply_deny_write_ace, has_allow_write_ace};
use crate::paths::sandbox_dir;
use crate::token::LocalSid;

const EVERYONE_SID: &str = "S-1-1-0";
const MAX_ITEMS_PER_DIR: usize = 64;
const MAX_SCAN_DEPTH: u32 = 2;

/// Default audit scan budget; the scan is best-effort and must never wedge setup.
pub const AUDIT_TIME_BUDGET: Duration = Duration::from_secs(10);

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AuditReport {
    /// Directories whose DACL allows Everyone any write-capable access.
    pub everyone_writable: Vec<PathBuf>,
    /// Number of directories inspected.
    pub scanned: usize,
    /// True when the time budget or per-dir caps cut the scan short.
    pub truncated: bool,
}

/// Scans `roots` (depth ≤ 2, ≤ 64 entries/dir, time-boxed) for directories
/// writable by Everyone.
pub fn scan_everyone_writable(roots: &[PathBuf], budget: Duration) -> Result<AuditReport> {
    let everyone = LocalSid::from_string(EVERYONE_SID)?;
    let deadline = Instant::now() + budget;
    let mut report = AuditReport::default();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        scan_dir(root, &everyone, 0, deadline, &mut report);
    }
    Ok(report)
}

fn scan_dir(
    dir: &Path,
    everyone: &LocalSid,
    depth: u32,
    deadline: Instant,
    report: &mut AuditReport,
) {
    if Instant::now() >= deadline {
        report.truncated = true;
        return;
    }
    report.scanned += 1;
    if has_allow_write_ace(dir, everyone.as_ptr()) {
        if !report.everyone_writable.iter().any(|p| p == dir) {
            report.everyone_writable.push(dir.to_path_buf());
        }
        // Children inherit the offending ACE in the common case; no need to
        // enumerate the whole subtree to repeat the same warning.
        return;
    }
    if depth >= MAX_SCAN_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for (index, entry) in entries.flatten().enumerate() {
        if index >= MAX_ITEMS_PER_DIR {
            report.truncated = true;
            break;
        }
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, everyone, depth + 1, deadline, report);
        }
    }
}

/// Pins a cap-SID deny-write ACE on each Everyone-writable offender
/// (best-effort backstop; failures are logged and skipped).
pub fn deny_write_on_offenders<F>(offenders: &[PathBuf], cap_sid: &LocalSid, mut log: F) -> usize
where
    F: FnMut(&str),
{
    let mut applied = 0;
    for path in offenders {
        match apply_deny_write_ace(path, cap_sid.as_ptr()) {
            Ok(true) => applied += 1,
            Ok(false) => {}
            Err(err) => log(&format!(
                "audit deny-write backstop: {} failed: {err}",
                path.display()
            )),
        }
    }
    applied
}

/// Writes the audit report to `.sandbox/audit_report.json` for support.
pub fn write_audit_report(home: &Path, report: &AuditReport) -> Result<PathBuf> {
    std::fs::create_dir_all(sandbox_dir(home))?;
    let path = sandbox_dir(home).join("audit_report.json");
    std::fs::write(&path, serde_json::to_string_pretty(report)?)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acl::revoke_ace;

    #[test]
    fn detects_everyone_writable_directory() {
        let dir =
            std::env::temp_dir().join(format!("zagens-audit-everyone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mk audit dir");

        let everyone = LocalSid::from_string(EVERYONE_SID).expect("everyone sid");
        crate::acl::apply_grant_write_ace(&dir, everyone.as_ptr()).expect("grant Everyone write");

        let report = scan_everyone_writable(std::slice::from_ref(&dir), Duration::from_secs(30))
            .expect("scan");
        assert!(
            report.everyone_writable.contains(&dir),
            "Everyone-writable dir must be reported: {report:?}"
        );

        revoke_ace(&dir, everyone.as_ptr());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clean_directory_not_reported() {
        let dir = std::env::temp_dir().join(format!("zagens-audit-clean-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mk clean dir");

        let report = scan_everyone_writable(std::slice::from_ref(&dir), Duration::from_secs(30))
            .expect("scan");
        assert!(
            !report.everyone_writable.contains(&dir),
            "clean temp dir reported as Everyone-writable: {report:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
