//! Validates user-chosen JSON export paths (H05) — blocks `..` and system directories.

use std::path::{Component, Path, PathBuf};

/// Canonical output path for `export_*_json` Tauri commands.
pub fn validate_export_json_path(save_path: &str) -> Result<PathBuf, String> {
    let trimmed = save_path.trim();
    if trimmed.is_empty() {
        return Err("保存路径不能为空".to_string());
    }
    if trimmed.contains("..") {
        return Err("路径不能包含 ..".to_string());
    }

    let raw = PathBuf::from(trimmed);
    if !raw.is_absolute() {
        return Err("导出路径必须是绝对路径".to_string());
    }

    let ext = raw.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !ext.eq_ignore_ascii_case("json") {
        return Err("仅允许导出 .json 文件".to_string());
    }

    let file_name = raw.file_name().ok_or_else(|| "无效的文件名".to_string())?;
    let parent = raw.parent().ok_or_else(|| "无效的父目录".to_string())?;

    let canon_parent = parent
        .canonicalize()
        .map_err(|e| format!("无法解析导出目录: {e}"))?;

    if export_parent_is_blocked(&canon_parent) {
        return Err("不允许写入系统目录".to_string());
    }

    for c in raw.components() {
        if matches!(c, Component::ParentDir) {
            return Err("路径不能包含 ..".to_string());
        }
    }

    Ok(canon_parent.join(file_name))
}

fn export_parent_is_blocked(canon_parent: &Path) -> bool {
    let lossy = canon_parent.to_string_lossy().to_lowercase();
    #[cfg(windows)]
    {
        for needle in [
            "\\windows\\",
            "\\windows",
            "\\program files\\",
            "\\program files (x86)\\",
            "\\programdata\\",
        ] {
            if lossy.contains(needle) {
                return true;
            }
        }
    }
    #[cfg(not(windows))]
    {
        for needle in ["/etc/", "/usr/bin", "/bin/", "/sbin/", "/proc/", "/sys/"] {
            if lossy.starts_with(needle) || lossy.contains(needle) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_dir_segments() {
        assert!(validate_export_json_path(r"C:\temp\..\evil.json").is_err());
    }

    #[test]
    fn requires_json_extension() {
        assert!(validate_export_json_path(r"C:\temp\out.txt").is_err());
    }
}
