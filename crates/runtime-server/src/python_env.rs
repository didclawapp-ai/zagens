//! Python runtime discovery and venv management.
//!
//! Provides:
//! - `find_python()` — locate a Python ≥3.8 interpreter (shared by RLM,
//!   `code_execution`, `write_office`).
//! - `resolve_python_for_office()` — interpreter for `write_office`: prefers
//!   the **bundled** PBS runtime (offline, deps pre-installed at app build).
//! - `ensure_office_venv()` — fallback: isolated venv under `~/.deepseek/office-py/`
//!   + `pip install` pinned deps (needs network on first setup).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// Serialize office venv creation across parallel tests / threads.
static OFFICE_VENV_LOCK: Mutex<()> = Mutex::new(());

/// Ordered candidate chains for Python discovery, per-platform.
#[cfg(windows)]
const PYTHON_CANDIDATES: &[&[&str]] = &[&["python3"], &["python"], &["py", "-3"]];
#[cfg(not(windows))]
const PYTHON_CANDIDATES: &[&[&str]] = &[&["python3"], &["python"]];

/// Minimum Python version required for venv + office libs (3.8).
const MIN_PYTHON_MAJOR: u16 = 3;
const MIN_PYTHON_MINOR: u16 = 8;

/// Office venv creation is unreliable on some CI images with Python 3.14+.
const MAX_OFFICE_VENV_MINOR: u16 = 13;

/// Office venv marker file — written after successful `pip install`.
const OFFICE_VENV_MARKER: &str = ".requirements-installed-v2";

/// Pinned requirements for the office venv (runtime fallback path).
///
/// When a bundled Python runtime is present (see `find_bundled_python`),
/// dependencies are pre-installed during the build and this list is
/// not used at runtime.  It is only consumed when building a system‑Python
/// venv as a fallback.
const OFFICE_REQUIREMENTS: &str = "\
python-docx==1.1.2
python-pptx==1.0.2
reportlab==4.2.5
";

// ── Discovery ───────────────────────────────────────────────────────────

/// Try to find a Python interpreter with version ≥ 3.8.
///
/// Returns `(binary_name, major, minor)` on success, or `None` if no
/// suitable Python was found.
pub fn find_python() -> Option<(String, u16, u16)> {
    // ── Path 1: Bundled Python (shipped with the app) ──
    if let Some(py) = find_bundled_python()
        && let Some(ver) = probe_python(&py.to_string_lossy(), &[])
        && (ver.0 > MIN_PYTHON_MAJOR || (ver.0 == MIN_PYTHON_MAJOR && ver.1 >= MIN_PYTHON_MINOR))
    {
        return Some((py.to_string_lossy().into_owned(), ver.0, ver.1));
    }

    // ── Path 2: System PATH scan ──
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

// ── Bundled Python discovery ────────────────────────────────────────────

fn python_bin_name() -> &'static str {
    #[cfg(windows)]
    {
        "python.exe"
    }
    #[cfg(target_os = "macos")]
    {
        "python3.12" // PBS macOS ships as "python3.12"
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "python3"
    }
}

/// Env var set by Zagens when spawning the runtime sidecar (absolute path to bundled `python`).
pub const ENV_BUNDLED_PYTHON: &str = "DEEPSEEK_BUNDLED_PYTHON";

/// Try to locate a bundled PBS Python runtime shipped alongside the binary.
///
/// **Zagens (Tauri):**
///   `DEEPSEEK_BUNDLED_PYTHON` (set by desktop shell)
///   `<app_dir>/Resources/python/python3.12`  (macOS)
///   `<app_dir>/python/python.exe`            (Windows/Linux)
///
/// **CLI/TUI standalone:**
///   `<exe_dir>/python-standalone/python-install/python(.exe)`
///
/// Returns the path to the python executable, or `None`.
pub fn find_bundled_python() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var(ENV_BUNDLED_PYTHON) {
        let path = PathBuf::from(raw.trim());
        if path.is_file() && probe_python(&path.to_string_lossy(), &[]).is_some() {
            return Some(path);
        }
    }

    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();

    let candidates = [
        // Tauri resource path on macOS
        exe_dir
            .join("../../Resources/python")
            .join(python_bin_name()),
        // Tauri resource path on Windows/Linux
        exe_dir.join("python").join(python_bin_name()),
        // Local dev / CLI sidecar
        exe_dir
            .join("python-standalone/python-install")
            .join(python_bin_name()),
    ];

    candidates
        .into_iter()
        .find(|path| path.is_file() && probe_python(&path.to_string_lossy(), &[]).is_some())
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

// ── WriteOffice interpreter ─────────────────────────────────────────────────

/// Verify bundled PBS has office wheels (build-time `prepare-python.mjs` lock).
fn verify_bundled_office_imports(python: &Path) -> Result<(), String> {
    let script = "import docx, pptx, reportlab, matplotlib, PIL; print('OK')";
    let output = Command::new(python)
        .args(["-c", script])
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .map_err(|e| format!("无法运行捆绑 Python: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "安装包内捆绑 Python 缺少办公依赖（docx/pptx/reportlab 等）。\
         请重新安装 Zagens 或联系支持。详情: {stderr}"
    ))
}

/// Python executable used by `write_office` for DOCX/PPTX/PDF generation.
///
/// **Bundled interpreter first** ([`find_bundled_python`]): Zagens / portable
/// builds ship PBS + wheels from `prepare-python.mjs`; no `pip` and no network.
///
/// Otherwise falls back to [`ensure_office_venv`] (PATH Python + first-time
/// `pip install`, typically needs internet) — dev / CLI only.
pub fn resolve_python_for_office() -> Result<PathBuf, String> {
    if let Some(py) = find_bundled_python() {
        verify_bundled_office_imports(&py)?;
        return Ok(py);
    }
    ensure_office_venv()
}

// ── venv management ─────────────────────────────────────────────────────

/// Resolve the office venv root directory (`~/.zagens/office-py/`).
pub fn office_venv_dir() -> Option<PathBuf> {
    zagens_config::user_data_path("office-py").ok()
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
fn find_python_for_office_venv() -> Option<(String, u16, u16)> {
    #[cfg(target_os = "macos")]
    {
        for cmd in ["python3.13", "python3.12", "python3.11", "python3"] {
            if let Some(ver) = probe_python(cmd, &[])
                && ver.0 == MIN_PYTHON_MAJOR
                && ver.1 >= MIN_PYTHON_MINOR
                && ver.1 <= MAX_OFFICE_VENV_MINOR
            {
                return Some((cmd.to_string(), ver.0, ver.1));
            }
        }
        return None;
    }
    #[cfg(not(target_os = "macos"))]
    {
        find_python().filter(|(_, major, minor)| {
            *major == MIN_PYTHON_MAJOR
                && *minor >= MIN_PYTHON_MINOR
                && *minor <= MAX_OFFICE_VENV_MINOR
        })
    }
}

pub fn ensure_office_venv() -> Result<PathBuf, String> {
    let _guard = OFFICE_VENV_LOCK
        .lock()
        .map_err(|e| format!("office venv lock poisoned: {e}"))?;

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

    // Need to create venv — Python from PATH / `py` launcher (bundled was
    // already tried in `resolve_python_for_office`).
    let (python_bin, major, minor) = find_python_for_office_venv().ok_or_else(|| {
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
        return Err(format!(
            "venv 创建后未找到解释器: {}",
            venv_python.display()
        ));
    }

    // pip install dependencies.
    // Write requirements to a temp file rather than piping through stdin:
    // `pip install -r -` (stdin) is unreliable on some Windows Python builds.
    let req_path = venv_dir.join("requirements-office-tmp.txt");
    std::fs::write(&req_path, OFFICE_REQUIREMENTS)
        .map_err(|e| format!("写入 requirements 文件失败: {e}"))?;

    let output = Command::new(&venv_python)
        .env("PYTHONIOENCODING", "utf-8")
        .args([
            "-m",
            "pip",
            "install",
            "--quiet",
            "--disable-pip-version-check",
        ])
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
