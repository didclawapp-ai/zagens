use deepseek_config::{ConfigStore, ConfigToml};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

/// Workspace `config.example.toml`, shipped as the canonical full default layout when
/// the desktop app saves an API key (matches upstream `Hmbown/DeepSeek-TUI`).
const CONFIG_EXAMPLE_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config.example.toml"
));

const PLACEHOLDER_TOP_API_KEY_LINE: &str =
    r#"api_key = "YOUR_DEEPSEEK_API_KEY" # must be non-empty"#;

/// Commented block under `[providers.deepseek]` in `config.example.toml`.
const PLACEHOLDER_DEEPSEEK_PROVIDER_BLOCK: &str = r#"# api_key = "YOUR_DEEPSEEK_API_KEY"
# base_url = "https://api.deepseek.com/beta"
# model = "deepseek-v4-pro""#;

fn escape_toml_basic_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() || c == '\u{7f}' => {
                use std::fmt::Write;
                let _ = write!(&mut out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Builds full on-disk config text: official example layout with DeepSeek key filled in
/// (top-level `api_key` plus active `[providers.deepseek]` credentials).
fn full_config_from_example_with_deepseek_key(key: &str) -> Result<String, String> {
    let esc = escape_toml_basic_string(key);
    let filled_top = format!(r#"api_key = "{esc}" # must be non-empty"#);
    let filled_provider = format!(
        r#"api_key = "{esc}"
base_url = "https://api.deepseek.com/beta"
model = "deepseek-v4-pro""#
    );

    let mut body = CONFIG_EXAMPLE_TEMPLATE.replace("\r\n", "\n");
    if !body.contains(PLACEHOLDER_TOP_API_KEY_LINE) {
        return Err(
            "internal: config template missing top-level DeepSeek api_key placeholder".to_string(),
        );
    }
    if !body.contains(PLACEHOLDER_DEEPSEEK_PROVIDER_BLOCK) {
        return Err(
            "internal: config template missing [providers.deepseek] placeholder block".to_string(),
        );
    }
    body = body.replace(PLACEHOLDER_DEEPSEEK_PROVIDER_BLOCK, &filled_provider);
    body = body.replace(PLACEHOLDER_TOP_API_KEY_LINE, &filled_top);

    if body.contains("YOUR_DEEPSEEK_API_KEY") {
        return Err(
            "internal: config template still contains DeepSeek placeholder after substitution"
                .to_string(),
        );
    }

    let _: ConfigToml =
        toml::from_str(&body).map_err(|e| format!("generated config failed validation: {e}"))?;

    Ok(body)
}

fn write_user_config_bytes(path: &Path, body: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    #[cfg(windows)]
    {
        // On Windows, verify the file is under USERPROFILE — config.toml
        // contains the user's API key and should stay within the user's
        // already-ACL-isolated home directory (#C1). If it isn't, still
        // write but log a security warning.
        let in_userprofile = std::env::var_os("USERPROFILE")
            .map(std::path::PathBuf::from)
            .is_some_and(|up| std::path::absolute(path).is_ok_and(|abs| abs.starts_with(&up)));
        if !in_userprofile {
            eprintln!(
                "deepseek-desktop: writing API key to {} which is outside USERPROFILE; \
                 consider moving config.toml to ~/.deepseek/",
                path.display()
            );
        }
        // Prevent other processes from reading the file while we write it.
        // After close, directory-inherited ACLs take effect (USERPROFILE
        // typically grants read only to the owner + SYSTEM).
        use std::os::windows::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .share_mode(0) // deny read/write/delete until we close
            .open(path)
            .map_err(|e| e.to_string())?;
        use std::io::Write;
        file.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
    }
    #[cfg(not(any(unix, windows)))]
    {
        fs::write(path, body).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct AppContext {
    pub runtime_port: u16,
    pub runtime_token: String,
    /// Wake the sidecar supervisor to restart `deepseek-tui`'s HTTP server (reload `config.toml`).
    pub sidecar_restart: Arc<Notify>,
    /// Signal the sidecar supervisor to shut down (kill the child process and exit).
    pub shutdown: Arc<Notify>,
}

#[derive(Debug, Serialize)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub version: String,
}

#[tauri::command]
pub async fn get_runtime_token(ctx: tauri::State<'_, AppContext>) -> Result<String, String> {
    Ok(ctx.runtime_token.clone())
}

#[tauri::command]
pub async fn get_runtime_port(ctx: tauri::State<'_, AppContext>) -> Result<u16, String> {
    Ok(ctx.runtime_port)
}

#[tauri::command]
pub async fn get_platform_info() -> Result<PlatformInfo, String> {
    Ok(PlatformInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[tauri::command]
pub async fn get_os_theme() -> Result<String, String> {
    Ok("dark".to_string())
}

#[derive(Debug, Serialize)]
pub struct ApiKeyStatus {
    pub configured: bool,
}

#[tauri::command]
pub fn get_api_key_status() -> Result<ApiKeyStatus, String> {
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let root = store
        .config
        .api_key
        .as_ref()
        .is_some_and(|s| !s.trim().is_empty());
    let nested = store
        .config
        .providers
        .deepseek
        .api_key
        .as_ref()
        .is_some_and(|s| !s.trim().is_empty());
    Ok(ApiKeyStatus {
        configured: root || nested,
    })
}

#[tauri::command]
pub fn save_deepseek_api_key(key: String, ctx: tauri::State<'_, AppContext>) -> Result<(), String> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("API key 不能为空".to_string());
    }
    let body = full_config_from_example_with_deepseek_key(&key)?;
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let path = store.path().to_path_buf();
    write_user_config_bytes(&path, &body)?;
    ctx.sidecar_restart.notify_one();
    Ok(())
}

#[cfg(test)]
mod save_config_tests {
    use super::*;

    #[test]
    fn full_template_substitution_parses_and_strips_placeholders() {
        let key = "sk-test\"\\\n\t\u{1f}x";
        let body = full_config_from_example_with_deepseek_key(key).expect("build config");
        assert!(!body.contains("YOUR_DEEPSEEK_API_KEY"));
        let parsed: ConfigToml = toml::from_str(&body).expect("parse");
        assert_eq!(parsed.api_key.as_deref(), Some(key));
        assert_eq!(parsed.providers.deepseek.api_key.as_deref(), Some(key));
        assert_eq!(
            parsed.providers.deepseek.base_url.as_deref(),
            Some("https://api.deepseek.com/beta")
        );
        assert_eq!(
            parsed.providers.deepseek.model.as_deref(),
            Some("deepseek-v4-pro")
        );
    }
}

#[tauri::command]
pub async fn get_locale() -> Result<String, String> {
    Ok("zh-CN".to_string())
}

// ---------------------------------------------------------------------------
// Binary file reader — used by the preview system for images, PDFs, and
// Office documents.  The runtime API (`/v1/threads/:id/workspace/file`)
// rejects non-UTF-8 content, so this Tauri command is the *only* path for
// binary preview.
//
// Security: resolves workspace root via the local runtime (`GET /v1/threads/:id`)
// and only reads paths validated against that root (same rules as
// `safe_thread_subpath` in `runtime_api.rs`).
// ---------------------------------------------------------------------------

const PREVIEW_MAX_BINARY_BYTES: u64 = 10 * 1024 * 1024; // 10 MB

#[derive(Serialize)]
pub struct BinaryFileResponse {
    pub mime_type: String,
    pub base64: String,
    pub size: u64,
    pub truncated: bool,
}

#[derive(Debug, Deserialize)]
struct ThreadDetailWire {
    thread: ThreadRecordWire,
}

#[derive(Debug, Deserialize)]
struct ThreadRecordWire {
    workspace: String,
}

/// Percent-encode a `{id}` path segment for `GET /v1/threads/{id}`.
fn percent_encode_path_segment(s: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(*b));
            }
            _ => {
                let _ = write!(&mut out, "%{b:02X}");
            }
        }
    }
    out
}

async fn fetch_thread_workspace_root(
    port: u16,
    token: &str,
    thread_id: &str,
) -> Result<PathBuf, String> {
    let enc = percent_encode_path_segment(thread_id.trim());
    if enc.is_empty() {
        return Err("thread_id 无效".to_string());
    }
    let url = format!("http://127.0.0.1:{port}/v1/threads/{enc}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|e| format!("HTTP 客户端: {e}"))?;

    let resp = client
        .get(&url)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| format!("无法连接运行时: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "无法获取线程工作区 (HTTP {})",
            resp.status().as_u16()
        ));
    }

    let detail: ThreadDetailWire = resp
        .json()
        .await
        .map_err(|e| format!("运行时响应无效: {e}"))?;

    let root = detail.thread.workspace.trim();
    if root.is_empty() {
        return Err("线程未配置工作区路径".to_string());
    }
    Ok(PathBuf::from(root))
}

const WORKSPACE_RESOLVE_WALK_MAX: usize = 12_000;
const WORKSPACE_RESOLVE_MATCH_MAX: usize = 24;

fn normalize_workspace_rel_query(rel: &str) -> String {
    rel.trim()
        .trim_start_matches(['/', '\\'])
        .replace('\\', "/")
}

fn workspace_suffix_walk_is_safe(suffix_norm: &str) -> bool {
    let parts: Vec<&str> = suffix_norm.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() >= 2 {
        return true;
    }
    if parts.len() == 1 {
        let name = parts[0];
        if matches!(
            name,
            "mod.rs" | "lib.rs" | "main.rs" | "index.ts" | "index.js" | "index.tsx" | "index.jsx"
        ) {
            return false;
        }
        return name.contains('.') && name.len() >= 5;
    }
    false
}

/// Try one relative path under workspace; `Ok(None)` = not an existing file.
fn try_file_under_workspace(workspace_root: &Path, rel: &str) -> Result<Option<PathBuf>, String> {
    let base = workspace_root
        .canonicalize()
        .map_err(|e| format!("工作区路径无效: {e}"))?;

    let trimmed = rel.trim().trim_start_matches(['/', '\\']);
    if trimmed.is_empty() {
        return Err("文件相对路径不能为空".to_string());
    }

    let rel_pb = PathBuf::from(trimmed);
    if rel_pb.is_absolute() {
        return Err("路径必须相对于工作区".to_string());
    }

    for c in rel_pb.components() {
        if matches!(c, Component::ParentDir) {
            return Err("路径不能包含 ..".to_string());
        }
    }

    let candidate = base.join(&rel_pb);
    let Ok(canon) = candidate.canonicalize() else {
        return Ok(None);
    };

    if !canon.starts_with(&base) {
        return Err("路径越出工作区".to_string());
    }

    if canon.is_file() {
        Ok(Some(canon))
    } else {
        Ok(None)
    }
}

fn resolve_under_workspace(workspace_root: &Path, rel: &str) -> Result<PathBuf, String> {
    let n = normalize_workspace_rel_query(rel);
    if n.is_empty() {
        return Err("文件相对路径不能为空".to_string());
    }
    if n.contains("..") {
        return Err("路径不能包含 ..".to_string());
    }

    let mut candidates: Vec<String> = Vec::new();
    candidates.push(n.clone());
    if !n.starts_with("src/") && !n.starts_with("crates/") && !n.starts_with("lib/") {
        candidates.push(format!("src/{n}"));
    }

    for c in &candidates {
        match try_file_under_workspace(workspace_root, c) {
            Ok(Some(p)) => return Ok(p),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
    }

    if !workspace_suffix_walk_is_safe(&n) {
        return Err(format!(
            "文件不存在或无法访问（已尝试: {}）",
            candidates.join(", ")
        ));
    }

    let base = workspace_root
        .canonicalize()
        .map_err(|e| format!("工作区路径无效: {e}"))?;
    let suffix_norm = n.trim_start_matches('/');
    let mut matches: Vec<PathBuf> = Vec::new();
    let walker = WalkBuilder::new(&base)
        .hidden(false)
        .git_ignore(true)
        .build();

    for (idx, entry) in walker.enumerate() {
        if idx > WORKSPACE_RESOLVE_WALK_MAX {
            break;
        }
        let entry = entry.map_err(|e| e.to_string())?;
        let ft = entry.file_type();
        if !ft.map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(&base) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str == suffix_norm || rel_str.ends_with(&format!("/{suffix_norm}")) {
            matches.push(path.to_path_buf());
            if matches.len() > WORKSPACE_RESOLVE_MATCH_MAX {
                return Err("路径匹配结果过多，请使用更具体的相对路径".to_string());
            }
        }
    }

    match matches.len() {
        0 => Err(format!(
            "文件不存在或无法访问（已尝试: {}）",
            candidates.join(", ")
        )),
        1 => Ok(matches[0].clone()),
        _ => {
            matches.sort_by_key(|p| {
                std::cmp::Reverse(
                    p.strip_prefix(&base)
                        .map(|x| x.components().count())
                        .unwrap_or(0),
                )
            });
            Ok(matches[0].clone())
        }
    }
}

fn read_binary_file_at(canonical_file: &Path) -> Result<BinaryFileResponse, String> {
    use base64::Engine;

    let meta = std::fs::metadata(canonical_file).map_err(|e| format!("无法获取文件信息: {e}"))?;
    let size = meta.len();
    let truncated = size > PREVIEW_MAX_BINARY_BYTES;
    let read_limit = if truncated {
        PREVIEW_MAX_BINARY_BYTES as usize
    } else {
        size as usize
    };

    let data = std::fs::read(canonical_file).map_err(|e| format!("无法读取文件: {e}"))?;
    let data = &data[..read_limit.min(data.len())];

    let mime_type = sniff_mime(data);
    let b64 = base64::engine::general_purpose::STANDARD.encode(data);

    Ok(BinaryFileResponse {
        mime_type,
        base64: b64,
        size,
        truncated,
    })
}

#[tauri::command]
pub async fn read_thread_workspace_binary(
    thread_id: String,
    relative_path: String,
    ctx: tauri::State<'_, AppContext>,
) -> Result<BinaryFileResponse, String> {
    let root =
        fetch_thread_workspace_root(ctx.runtime_port, &ctx.runtime_token, &thread_id).await?;
    let path = resolve_under_workspace(&root, &relative_path)?;
    read_binary_file_at(&path)
}

#[tauri::command]
pub fn read_workspace_binary_at_root(
    workspace_root: String,
    relative_path: String,
) -> Result<BinaryFileResponse, String> {
    let trimmed = workspace_root.trim();
    if trimmed.is_empty() {
        return Err("工作区路径不能为空".to_string());
    }
    let root = PathBuf::from(trimmed);
    let path = resolve_under_workspace(&root, &relative_path)?;
    read_binary_file_at(&path)
}

fn sniff_mime(data: &[u8]) -> String {
    if data.len() >= 8 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
        return "image/png".into();
    }
    if data.len() >= 3 && &data[0..3] == b"\xff\xd8\xff" {
        return "image/jpeg".into();
    }
    if data.len() >= 4 && &data[0..4] == b"GIF8" {
        return "image/gif".into();
    }
    if data.len() >= 4 && &data[0..4] == b"RIFF" && data.len() >= 12 && &data[8..12] == b"WEBP" {
        return "image/webp".into();
    }
    if data.len() >= 4 && &data[0..4] == b"<svg" {
        return "image/svg+xml".into();
    }
    if data.len() >= 2 && &data[0..2] == b"BM" {
        return "image/bmp".into();
    }
    if data.len() >= 4 && &data[0..4] == b"PK\x03\x04" {
        // ZIP-based formats: docx, xlsx, pptx are all ZIP archives
        // For now, return a generic Office MIME — the frontend
        // dispatches to OfficePlaceholder.
        return "application/zip".into();
    }
    if data.len() >= 4 && &data[0..4] == b"%PDF" {
        return "application/pdf".into();
    }
    "application/octet-stream".into()
}

// ---------------------------------------------------------------------------
// open_in_shell — opens a directory in the system file manager
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn open_in_shell(path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(trimmed)
            .spawn()
            .map_err(|e| format!("无法打开文件管理器: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| format!("无法打开文件管理器: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| format!("无法打开文件管理器: {e}"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// export_thread_json — fetches thread from runtime and writes to a file
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn export_thread_json(
    thread_id: String,
    save_path: String,
    ctx: tauri::State<'_, AppContext>,
) -> Result<(), String> {
    let enc = percent_encode_path_segment(thread_id.trim());
    if enc.is_empty() {
        return Err("thread_id 无效".to_string());
    }
    let url = format!("http://127.0.0.1:{}/v1/threads/{}", ctx.runtime_port, enc);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP 客户端: {e}"))?;

    let resp = client
        .get(&url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", ctx.runtime_token),
        )
        .send()
        .await
        .map_err(|e| format!("无法连接运行时: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "无法获取线程数据 (HTTP {})",
            resp.status().as_u16()
        ));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("运行时响应无效: {e}"))?;

    let json = serde_json::to_string_pretty(&body).map_err(|e| format!("JSON 序列化失败: {e}"))?;

    std::fs::write(&save_path, json).map_err(|e| format!("保存失败: {e}"))?;

    Ok(())
}

#[tauri::command]
pub fn restart_sidecar(ctx: tauri::State<'_, AppContext>) -> Result<(), String> {
    ctx.sidecar_restart.notify_one();
    Ok(())
}

// ---------------------------------------------------------------------------
// export_session_json — GET /v1/sessions/{id} and write pretty JSON (desktop UX)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn export_session_json(
    session_id: String,
    save_path: String,
    ctx: tauri::State<'_, AppContext>,
) -> Result<(), String> {
    let enc = percent_encode_path_segment(session_id.trim());
    if enc.is_empty() {
        return Err("session_id 无效".to_string());
    }
    let url = format!("http://127.0.0.1:{}/v1/sessions/{}", ctx.runtime_port, enc);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP 客户端: {e}"))?;

    let resp = client
        .get(&url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", ctx.runtime_token),
        )
        .send()
        .await
        .map_err(|e| format!("无法连接运行时: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "无法获取会话数据 (HTTP {})",
            resp.status().as_u16()
        ));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("运行时响应无效: {e}"))?;

    let json = serde_json::to_string_pretty(&body).map_err(|e| format!("JSON 序列化失败: {e}"))?;

    std::fs::write(&save_path, json).map_err(|e| format!("保存失败: {e}"))?;

    Ok(())
}
