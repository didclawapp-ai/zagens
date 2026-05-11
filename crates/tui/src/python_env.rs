//! Python runtime discovery and venv management.
//!
//! Provides:
//! - `find_python()` — locate a Python ≥3.8 interpreter (shared by RLM,
//!   `code_execution`, `write_office`).
//! - `ensure_office_venv()` — create/manage `~/.deepseek/office-py/` venv
//!   with pinned `python-docx`, `python-pptx` deps for `WriteOfficeTool`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Ordered candidate chains for Python discovery, per-platform.
#[cfg(windows)]
const PYTHON_CANDIDATES: &[&[&str]] = &[&["python3"], &["python"], &["py", "-3"]];
#[cfg(not(windows))]
const PYTHON_CANDIDATES: &[&[&str]] = &[&["python3"], &["python"]];

/// Minimum Python version required for venv + office libs (3.8).
const MIN_PYTHON_MAJOR: u16 = 3;
const MIN_PYTHON_MINOR: u16 = 8;

/// Office venv marker file — written after successful `pip install`.
const OFFICE_VENV_MARKER: &str = ".requirements-installed-v1";

/// Pinned requirements for the office venv.
const OFFICE_REQUIREMENTS: &str = "\
python-docx==1.1.2
python-pptx==1.0.2
";

// ── Discovery ───────────────────────────────────────────────────────────

/// Try to find a Python interpreter with version ≥ 3.8.
///
/// Returns `(binary_name, major, minor)` on success, or `None` if no
/// suitable Python was found.
pub fn find_python() -> Option<(String, u16, u16)> {
    for args in PYTHON_CANDIDATES {
        let (bin, extra) = (args[0], &args[1..]);
        if let Some(ver) = probe_python(bin, extra) {
            if ver.0 > MIN_PYTHON_MAJOR || (ver.0 == MIN_PYTHON_MAJOR && ver.1 >= MIN_PYTHON_MINOR)
            {
                return Some((bin.to_string(), ver.0, ver.1));
            }
        }
    }
    None
}

/// Run `python -c "import sys; print(sys.version_info[:2])"` and parse the
/// `(major, minor)` tuple.
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

/// Parse `(major, minor)` from output like `(3, 11)` or `(3, 11)\r\n`.
fn parse_version_tuple(s: &str) -> Option<(u16, u16)> {
    let s = s.trim();
    let inner = s.trim_start_matches('(').trim_end_matches(')');
    let mut parts = inner.split(',');
    let major: u16 = parts.next()?.trim().parse().ok()?;
    let minor: u16 = parts.next()?.trim().parse().ok()?;
    Some((major, minor))
}

// ── venv management ─────────────────────────────────────────────────────

/// Resolve the office venv root directory (`~/.deepseek/office-py/`).
pub fn office_venv_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(".deepseek").join("office-py"))
}

/// Path to the venv's Python interpreter (platform-aware).
fn office_venv_python(venv_dir: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        venv_dir.join("Scripts").join("python.exe")
    }
    #[cfg(not(windows))]
    {
        venv_dir.join("bin").join("python")
    }
}

/// Ensure the office venv exists and has dependencies installed.
///
/// Returns the path to the venv's Python interpreter, or an error string
/// suitable for a `ToolResult`.
pub fn ensure_office_venv() -> Result<PathBuf, String> {
    let venv_dir =
        office_venv_dir().ok_or_else(|| "无法确定 home 目录，无法创建 office venv".to_string())?;

    // Already installed?
    let marker = venv_dir.join(OFFICE_VENV_MARKER);
    if marker.exists() {
        let py = office_venv_python(&venv_dir);
        if py.exists() {
            return Ok(py);
        }
    }

    // Need to create venv — find system Python first.
    let (python_bin, major, minor) =
        find_python().ok_or_else(|| {
            "未找到 Python ≥ 3.8。请安装 Python 后重试。\n\
             下载: https://www.python.org/downloads/\n\
             Windows 用户也可通过 `winget install Python.Python.3.12` 安装"
                .to_string()
        })?;

    // Create venv.
    let _ = std::fs::create_dir_all(venv_dir.parent().unwrap());
    let status = Command::new(&python_bin)
        .args(["-m", "venv"])
        .arg(&venv_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .map_err(|e| format!("创建 venv 失败: {e}"))?;

    if !status.success() {
        return Err(format!(
            "创建 venv 失败 (Python {major}.{minor})。请检查 Python 安装。"
        ));
    }

    let venv_python = office_venv_python(&venv_dir);
    if !venv_python.exists() {
        return Err(format!("venv 创建后未找到解释器: {}", venv_python.display()));
    }

    // pip install dependencies.
    // Write requirements to a temp file rather than piping through stdin:
    // `pip install -r -` (stdin) is unreliable on some Windows Python builds.
    let req_path = venv_dir.join("requirements-office-tmp.txt");
    std::fs::write(&req_path, OFFICE_REQUIREMENTS)
        .map_err(|e| format!("写入 requirements 文件失败: {e}"))?;

    let output = Command::new(&venv_python)
        .env("PYTHONIOENCODING", "utf-8")
        .args(["-m", "pip", "install", "--quiet", "--disable-pip-version-check"])
        .args(["-r"])
        .arg(&req_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("启动 pip install 失败: {e}"))?;

    // Best-effort cleanup — don't fail if this file can't be removed.
    let _ = std::fs::remove_file(&req_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("pip install 依赖失败:\n{stderr}"));
    }

    // Write marker.
    std::fs::write(&marker, "1").map_err(|e| format!("写入 venv 标记文件失败: {e}"))?;

    Ok(venv_python)
}

// ── Tests ────────────────────────────────────────────────────────────────

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
