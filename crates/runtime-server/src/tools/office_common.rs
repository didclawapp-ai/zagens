//! Shared helpers for office read/write tools (paths, payload cache, previews, errors).

use calamine::{Data, Reader, open_workbook_auto};
use chrono::Local;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use super::file::MAX_FILE_SIZE;
use super::spec::{ToolContext, ToolError, optional_str};

pub const DELIVERABLES_DIR: &str = "deliverables";
pub const OFFICE_CACHE_DIR: &str = "deliverables/.office";
const PREVIEW_MAX_ROWS: usize = 200;

/// Resolve output path: explicit `path` or default under `deliverables/`.
pub fn resolve_write_office_path(
    input: &Value,
    context: &ToolContext,
    format: &str,
) -> Result<PathBuf, ToolError> {
    if let Some(path_str) = optional_str(input, "path") {
        return context.resolve_path(path_str);
    }
    let title = optional_str(input, "title");
    let filename = unique_office_filename(&context.workspace, format, title)?;
    let rel = format!("{DELIVERABLES_DIR}/{filename}");
    context.resolve_path(&rel)
}

/// Sanitize title → safe filename stem (no extension).
pub fn sanitize_office_stem(title: &str) -> String {
    let mut out = String::new();
    for c in title.trim().chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else if c.is_whitespace() || matches!(c, '，' | '。' | '/' | '\\') {
            if !out.ends_with('-') && !out.is_empty() {
                out.push('-');
            }
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "office-doc".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

fn unique_office_filename(
    workspace: &Path,
    format: &str,
    title: Option<&str>,
) -> Result<String, ToolError> {
    let ext = format;
    let stem = title
        .map(sanitize_office_stem)
        .unwrap_or_else(|| {
            Local::now()
                .format("office-{format}-%Y%m%d-%H%M%S")
                .to_string()
        });
    let deliverables = workspace.join(DELIVERABLES_DIR);
    fs::create_dir_all(&deliverables).map_err(|e| {
        ToolError::execution_failed(format!("无法创建 {DELIVERABLES_DIR}/: {e}"))
    })?;

    let candidate = format!("{stem}.{ext}");
    if !deliverables.join(&candidate).exists() {
        return Ok(candidate);
    }
    for n in 2..=99 {
        let candidate = format!("{stem}-{n}.{ext}");
        if !deliverables.join(&candidate).exists() {
            return Ok(candidate);
        }
    }
    let candidate = format!(
        "{stem}-{}.{}",
        Local::now().format("%H%M%S"),
        ext
    );
    Ok(candidate)
}

/// Cache dir + payload path for a written office file.
pub fn office_payload_cache_path(workspace: &Path, output: &Path) -> Result<PathBuf, ToolError> {
    let file_name = output
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ToolError::execution_failed("无效输出文件名"))?;
    let cache_dir = workspace.join(OFFICE_CACHE_DIR);
    fs::create_dir_all(&cache_dir).map_err(|e| {
        ToolError::execution_failed(format!("无法创建 {OFFICE_CACHE_DIR}/: {e}"))
    })?;
    Ok(cache_dir.join(format!("{file_name}.payload.json")))
}

/// Persist generation payload (minus `path`) for iterative edits.
pub fn save_office_payload(
    workspace: &Path,
    output: &Path,
    input: &Value,
) -> Result<PathBuf, ToolError> {
    let cache_path = office_payload_cache_path(workspace, output)?;
    let mut payload = input.clone();
    if let Some(obj) = payload.as_object_mut() {
        obj.remove("path");
    }
    let bytes = serde_json::to_vec_pretty(&payload)
        .map_err(|e| ToolError::execution_failed(format!("序列化 payload 失败: {e}")))?;
    fs::write(&cache_path, bytes).map_err(|e| {
        ToolError::execution_failed(format!("写入 payload 缓存失败 ({}): {e}", cache_path.display()))
    })?;
    Ok(cache_path)
}

/// Load cached payload for an office output file.
pub fn load_office_payload_file(workspace: &Path, office_path: &Path) -> Result<Value, ToolError> {
    let cache_path = office_payload_cache_path(workspace, office_path)?;
    if !cache_path.is_file() {
        return Err(ToolError::execution_failed(format!(
            "未找到生成缓存 {}。仅对通过 write_office 生成的文件可用；请用 read_office 读取后全量重写，或重新生成。",
            cache_path.display()
        )));
    }
    let bytes = fs::read(&cache_path).map_err(|e| {
        ToolError::execution_failed(format!("读取 payload 缓存失败: {e}"))
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|e| ToolError::execution_failed(format!("解析 payload 缓存失败: {e}")))
}

/// Classify Python / office generation failures for the model and UI.
pub fn classify_office_generation_error(raw: &str, stderr: &str) -> String {
    let combined = format!("{raw}\n{stderr}").to_ascii_lowercase();
    if raw.contains("超时") || combined.contains("timeout") {
        return format!("[OFFICE_TIMEOUT] {raw}");
    }
    if combined.contains("modulenotfounderror")
        || combined.contains("no module named")
        || combined.contains("import error")
    {
        return format!(
            "[OFFICE_DEPS] Office Python 依赖缺失。请在设置中查看 Office 环境，或等待首次 venv 安装完成。\n{raw}"
        );
    }
    if combined.contains("permission denied") || combined.contains("access is denied") {
        return format!("[OFFICE_PERMISSION] 无写入权限: {raw}");
    }
    if combined.contains("resolve_python") || combined.contains("python") && combined.contains("not found")
    {
        return format!("[OFFICE_PYTHON] 未找到可用的 Python 运行时: {raw}");
    }
    if combined.contains("network") || combined.contains("pip install") {
        return format!("[OFFICE_NETWORK] 首次安装 Office 依赖需要网络: {raw}");
    }
    format!("[OFFICE_ERROR] {raw}")
}

/// Write HTML table preview sidecar for XLSX (`.preview.html` next to payload cache).
pub fn write_xlsx_html_preview(workspace: &Path, xlsx_path: &Path) -> Result<Option<PathBuf>, String> {
    let meta = fs::metadata(xlsx_path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_FILE_SIZE {
        return Ok(None);
    }
    let file_name = xlsx_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "invalid xlsx name".to_string())?;
    let cache_dir = workspace.join(OFFICE_CACHE_DIR);
    fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;
    let preview_path = cache_dir.join(format!("{file_name}.preview.html"));

    let mut workbook =
        open_workbook_auto(xlsx_path).map_err(|e| format!("preview open failed: {e}"))?;
    let sheet_names = workbook.sheet_names().to_vec();
    let sheet_name = sheet_names.first().cloned().unwrap_or_else(|| "Sheet1".to_string());
    let range = workbook
        .worksheet_range(&sheet_name)
        .map_err(|e| format!("preview range failed: {e}"))?;
    let (height, width) = range.get_size();
    let rows = height.min(PREVIEW_MAX_ROWS);

    let mut html = String::from(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><style>\
         body{font:13px/1.4 system-ui,sans-serif;margin:12px}\
         table{border-collapse:collapse;width:100%}\
         th,td{border:1px solid #ccc;padding:4px 8px;text-align:left}\
         th{background:#f0f0f0}</style></head><body>",
    );
    html.push_str(&format!(
        "<p><strong>{sheet_name}</strong> — 预览前 {rows} 行（共 {height} 行）</p><table>"
    ));
    for row_idx in 0..rows {
        html.push_str(if row_idx == 0 { "<thead><tr>" } else if row_idx == 1 {
            "</tr></thead><tbody><tr>"
        } else {
            "<tr>"
        });
        for col_idx in 0..width {
            let cell = range.get((row_idx, col_idx)).unwrap_or(&Data::Empty);
            let text = escape_html(&format_data_preview(cell));
            html.push_str(&format!("<td>{text}</td>"));
        }
        html.push_str("</tr>");
    }
    if rows > 0 {
        html.push_str("</tbody>");
    }
    html.push_str("</table></body></html>");
    fs::write(&preview_path, html).map_err(|e| e.to_string())?;
    Ok(Some(preview_path))
}

fn format_data_preview(data: &Data) -> String {
    match data {
        Data::Empty => String::new(),
        Data::Float(f) => format!("{f}"),
        Data::Int(i) => i.to_string(),
        Data::String(s) => s.clone(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("#{e:?}"),
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Relative path from workspace root (POSIX slashes).
pub fn workspace_rel_path(workspace: &Path, file: &Path) -> String {
    file.strip_prefix(workspace)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn sanitize_stem_basic() {
        assert_eq!(sanitize_office_stem("Weekly Report 2025"), "Weekly-Report-2025");
    }

    #[test]
    fn unique_filename_avoids_collision() {
        let dir = tempdir().unwrap();
        let d = dir.path().join(DELIVERABLES_DIR);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("report.docx"), b"x").unwrap();
        let name = unique_office_filename(dir.path(), "docx", Some("report")).unwrap();
        assert_eq!(name, "report-2.docx");
    }
}
