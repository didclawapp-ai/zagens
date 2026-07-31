//! Python runtime discovery for RLM and `code_execution` (system PATH only).

use std::process::Command;

/// Ordered candidate chains for Python discovery, per-platform.
#[cfg(windows)]
const PYTHON_CANDIDATES: &[&[&str]] = &[&["python3"], &["python"], &["py", "-3"]];
#[cfg(not(windows))]
const PYTHON_CANDIDATES: &[&[&str]] = &[&["python3"], &["python"]];

/// Minimum Python version required (3.8).
const MIN_PYTHON_MAJOR: u16 = 3;
const MIN_PYTHON_MINOR: u16 = 8;

/// Try to find a Python interpreter with version ≥ 3.8 on `PATH`.
///
/// Returns `(binary_name, major, minor)` on success, or `None` if no
/// suitable Python was found. Desktop no longer ships a bundled PBS runtime.
pub fn find_python() -> Option<(String, u16, u16)> {
    for args in PYTHON_CANDIDATES {
        let (bin, extra) = (args[0], &args[1..]);
        if let Some(ver) = probe_python(bin, extra)
            && (ver.0 > MIN_PYTHON_MAJOR
                || (ver.0 == MIN_PYTHON_MAJOR && ver.1 >= MIN_PYTHON_MINOR))
        {
            return Some((bin.to_string(), ver.0, ver.1));
        }
    }
    None
}

fn probe_python(binary: &str, extra_args: &[&str]) -> Option<(u16, u16)> {
    let mut cmd = Command::new(binary);
    cmd.args(extra_args)
        .args(["-c", "import sys; print(sys.version_info[:2])"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_version_tuple(stdout.trim())
}

fn parse_version_tuple(s: &str) -> Option<(u16, u16)> {
    let s = s.trim();
    let inner = s.trim_start_matches('(').trim_end_matches(')');
    let mut parts = inner.split(',');
    let major: u16 = parts.next()?.trim().parse().ok()?;
    let minor: u16 = parts.next()?.trim().parse().ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_tuple() {
        assert_eq!(parse_version_tuple("(3, 11)"), Some((3, 11)));
        assert_eq!(parse_version_tuple("(3, 8)"), Some((3, 8)));
        assert_eq!(parse_version_tuple("(3, 12)\r\n"), Some((3, 12)));
        assert_eq!(parse_version_tuple(""), None);
        assert_eq!(parse_version_tuple("garbage"), None);
    }
}
