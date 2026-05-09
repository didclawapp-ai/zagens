use deepseek_config::{ConfigStore, ConfigToml};
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
pub struct AppContext {
    pub runtime_port: u16,
    pub runtime_token: String,
    /// Wake the sidecar supervisor to restart `deepseek-tui`'s HTTP server (reload `config.toml`).
    pub sidecar_restart: Arc<Notify>,
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

/// Resolve `relative_path` under `workspace_root`, forbid `..` and escapes
/// (mirrors `safe_thread_subpath` in `crates/tui/src/runtime_api.rs`).
fn resolve_under_workspace(workspace_root: &Path, rel: &str) -> Result<PathBuf, String> {
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
    let canon = candidate
        .canonicalize()
        .map_err(|_| format!("文件不存在或无法访问: {}", candidate.display()))?;

    if !canon.starts_with(&base) {
        return Err("路径越出工作区".to_string());
    }

    Ok(canon)
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
