use deepseek_config::{
    ConfigStore, ConfigToml, DEFAULT_VISION_MODEL, vision_should_check_degenerate_ocr_template,
    vision_user_prompt_for_model,
};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

/// reqwest 顶层 `Display` 常为笼统的「error sending request」，展开 `source()` 链便于跨机排查。
fn chain_transport_error_cn<E: Error + Send + Sync>(prefix: &str, err: &E) -> String {
    let mut msg = format!("{prefix}: {err}");
    let mut cur = err.source();
    while let Some(next) = cur {
        msg.push_str(" → ");
        msg.push_str(&next.to_string());
        cur = next.source();
    }
    msg.push_str(
        "。若在个别电脑上出现：请在同一台机器用浏览器或 PowerShell/`curl` 访问该 API；检查防火墙或杀毒拦截、办公网代理（可设置环境变量 HTTPS_PROXY）、DNS 是否正常、以及地区网络是否能访问 siliconflow.cn。",
    );
    msg
}

/// Merges DeepSeek credentials into an existing [`ConfigToml`] without
/// overwriting unrelated tables (e.g. `[vision]`).
#[allow(dead_code)]
fn merge_deepseek_api_key(config: &mut ConfigToml, key: &str) {
    config.api_key = Some(key.to_string());
    config.providers.deepseek.api_key = Some(key.to_string());
    if config.providers.deepseek.base_url.is_none() {
        config.providers.deepseek.base_url = Some("https://api.deepseek.com/beta".to_string());
    }
    if config.providers.deepseek.model.is_none() {
        config.providers.deepseek.model = Some("deepseek-v4-pro".to_string());
    }
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
    let secrets = deepseek_secrets::Secrets::auto_detect();
    let configured = secrets.resolve("deepseek").is_some();
    Ok(ApiKeyStatus { configured })
}

#[tauri::command]
pub fn save_deepseek_api_key(key: String, ctx: tauri::State<'_, AppContext>) -> Result<(), String> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("API key 不能为空".to_string());
    }

    // Write to OS keyring first — if this fails we do NOT clear config.toml
    let secrets = deepseek_secrets::Secrets::auto_detect();
    secrets
        .set("deepseek", &key)
        .map_err(|e| format!("无法保存到系统密钥链: {e}"))?;

    // Remove plaintext key from config.toml; keep provider section structure
    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    store.config.api_key = None;
    store.config.providers.deepseek.api_key = None;
    if store.config.providers.deepseek.base_url.is_none() {
        store.config.providers.deepseek.base_url =
            Some("https://api.deepseek.com/beta".to_string());
    }
    if store.config.providers.deepseek.model.is_none() {
        store.config.providers.deepseek.model = Some("deepseek-v4-pro".to_string());
    }
    store.save().map_err(|e| e.to_string())?;

    ctx.sidecar_restart.notify_one();
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct VisionBridgeStatus {
    pub configured: bool,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

#[tauri::command]
pub fn get_vision_bridge_status() -> Result<VisionBridgeStatus, String> {
    let secrets = deepseek_secrets::Secrets::auto_detect();
    let configured = secrets.resolve("vision").is_some();
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let v = store.config.vision.as_ref();
    Ok(VisionBridgeStatus {
        configured,
        base_url: v
            .and_then(|x| x.base_url.clone())
            .filter(|s| !s.trim().is_empty()),
        model: v
            .and_then(|x| x.model.clone())
            .filter(|s| !s.trim().is_empty()),
    })
}

#[tauri::command]
pub fn save_vision_bridge(
    api_key: String,
    base_url: String,
    model: String,
    ctx: tauri::State<'_, AppContext>,
) -> Result<(), String> {
    let key_trim = api_key.trim();
    if key_trim.is_empty() {
        // If no new key is provided, check keyring for existing one
        let secrets = deepseek_secrets::Secrets::auto_detect();
        if secrets.resolve("vision").is_none() {
            return Err(
                "请填写视觉桥接 API Key；密钥保存后不会回显。修改端点或模型时也需要重新输入密钥。"
                    .to_string(),
            );
        }
    } else {
        let secrets = deepseek_secrets::Secrets::auto_detect();
        secrets
            .set("vision", key_trim)
            .map_err(|e| format!("无法保存视觉桥接密钥到系统密钥链: {e}"))?;
    }

    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let mut v = store.config.vision.clone().unwrap_or_default();
    // Never persist the key in config.toml
    v.api_key = None;

    let bu = base_url.trim();
    v.base_url = if bu.is_empty() {
        None
    } else {
        Some(bu.to_string())
    };
    let m = model.trim();
    v.model = if m.is_empty() { None } else { Some(m.to_string()) };

    store.config.vision = Some(v);
    store.save().map_err(|e| e.to_string())?;
    ctx.sidecar_restart.notify_one();
    Ok(())
}

#[tauri::command]
pub fn clear_vision_bridge(ctx: tauri::State<'_, AppContext>) -> Result<(), String> {
    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    store.config.vision = None;
    store.save().map_err(|e| e.to_string())?;
    // Also clear from keyring
    let secrets = deepseek_secrets::Secrets::auto_detect();
    secrets.delete("vision").ok();
    ctx.sidecar_restart.notify_one();
    Ok(())
}

/// Rejects pathological DeepSeek-OCR output (repeated template clauses) when the wrong
/// user prompt was used. Only applied for DeepSeek-OCR models.
fn reject_known_degenerate_ocr_output(text: &str) -> Result<(), String> {
    let marker = "如果图中包含表格，请用表格形式输出";
    if text.matches(marker).count() >= 2 {
        return Err(
            "视觉模型输出为无效重复模板句式。若使用 DeepSeek-OCR，请在硅基流动文档中采用官方 `<image>` + `<|grounding|>` 提示词后重试。"
                .to_string(),
        );
    }
    Ok(())
}

/// Some OpenAI-compatible vision providers return `message.content` as a string,
/// others as an array of `{ "type":"text","text":"..." }` parts.
fn coerce_chat_completion_message_text(content: &serde_json::Value) -> Result<String, String> {
    if let Some(s) = content.as_str() {
        return Ok(s.to_string());
    }
    let Some(parts) = content.as_array() else {
        return Err("视觉桥接响应 message.content 格式无法识别".to_string());
    };
    let mut out: Vec<String> = Vec::new();
    for item in parts {
        if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
            if !t.trim().is_empty() {
                out.push(t.to_string());
            }
        }
    }
    if out.is_empty() {
        return Err(
            "视觉桥接返回的 message.content 中无可读取的文本段落（可能为暂未支持的提供商格式）"
                .to_string(),
        );
    }
    Ok(out.join("\n"))
}

/// Runs the configured OpenAI-compatible vision bridge on a `data:image/...;base64,...` URL.
/// Used by the web UI composer before sending user text to the main DeepSeek model.
#[tauri::command]
pub async fn vision_transcribe_image(data_url: String) -> Result<String, String> {
    let data_url = data_url.trim();
    if !data_url.starts_with("data:image/") {
        return Err("仅支持 data:image/…;base64,… 格式的图片".to_string());
    }
    let b64_part = data_url
        .split_once(";base64,")
        .map(|x| x.1)
        .ok_or_else(|| "无效的 data URL（缺少 ;base64,）".to_string())?;
    let approx_bytes = (b64_part.len().saturating_mul(3)) / 4;
    if approx_bytes > 20 * 1024 * 1024 {
        return Err("图片过大（解码后约超过 20 MB）".to_string());
    }

    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let vision = store.config.vision.as_ref().ok_or_else(|| {
        "未配置视觉桥接：请在 设置 → API Key 中保存视觉桥接密钥".to_string()
    })?;
    // Key from OS keyring first, then config.toml fallback (legacy plaintext)
    let secrets = deepseek_secrets::Secrets::auto_detect();
    let api_key = secrets.resolve("vision").or_else(|| {
        vision
            .api_key
            .as_ref()
            .filter(|s| !s.trim().is_empty())
            .cloned()
    }).ok_or_else(|| "未配置视觉桥接 API Key".to_string())?;
    let base_url = vision
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("https://api.siliconflow.cn/v1");
    let base_url = base_url.trim_end_matches('/');
    let model = vision
        .model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_VISION_MODEL);
    let user_prompt = vision_user_prompt_for_model(model);

    let body = serde_json::json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "image_url", "image_url": {"url": data_url, "detail": "high"}},
                {"type": "text", "text": user_prompt}
            ]
        }],
        "max_tokens": 4096,
        "temperature": 0.0,
        "stream": false,
    });

    let timeout_secs = std::env::var("VISION_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&t| t >= 30)
        .unwrap_or(120)
        .min(600); // hard cap 10 minutes
    let mut client_builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(timeout_secs));

    // reqwest with rustls backend does not auto-detect system proxies.
    // Respect HTTPS_PROXY / ALL_PROXY / HTTP_PROXY manually.
    let proxy_url = std::env::var("HTTPS_PROXY")
        .or_else(|_| std::env::var("https_proxy"))
        .or_else(|_| std::env::var("ALL_PROXY"))
        .or_else(|_| std::env::var("all_proxy"))
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .or_else(|_| std::env::var("http_proxy"))
        .ok()
        .filter(|s| !s.trim().is_empty());
    if let Some(ref url) = proxy_url {
        match reqwest::Proxy::all(url) {
            Ok(proxy) => {
                client_builder = client_builder.proxy(proxy);
            }
            Err(e) => {
                eprintln!("[vision] 代理 URL 解析失败 ({url}): {e}，将直连");
            }
        }
    }

    let client = client_builder
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

    let body_bytes = serde_json::to_vec(&body).map_err(|e| format!("序列化请求失败: {e}"))?;
    let body_mb = body_bytes.len() as f64 / (1024.0 * 1024.0);
    let upload_est_seconds = |mbps: f64| -> f64 { (body_bytes.len() as f64 * 8.0) / (mbps * 1_000_000.0) };
    eprintln!(
        "[vision] POST {}/chat/completions  body={:.1} MB  model={}  timeout={}s \
         上传耗时估算: 10Mbps≈{:.1}s 5Mbps≈{:.1}s 2Mbps≈{:.1}s 1Mbps≈{:.1}s",
        base_url, body_mb, model, timeout_secs,
        upload_est_seconds(10.0), upload_est_seconds(5.0), upload_est_seconds(2.0), upload_est_seconds(1.0),
    );

    let resp = client
        .post(format!("{base_url}/chat/completions"))
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .header("Content-Type", "application/json")
        .body(body_bytes)
        .send()
        .await
        .map_err(|e| chain_transport_error_cn("视觉桥接 HTTP 请求失败", &e))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析视觉桥接响应失败: {e}"))?;

    if !status.is_success() {
        let msg = resp_body
            .get("message")
            .and_then(|v| v.as_str())
            .or_else(|| resp_body.get("error").and_then(|e| e.as_str()))
            .unwrap_or("unknown error");
        return Err(format!("视觉桥接返回错误 (HTTP {status}): {msg}"));
    }

    let content_raw = resp_body["choices"].get(0).and_then(|c| c.get("message")).and_then(|m| m.get("content")).ok_or_else(|| {
        "视觉桥接响应格式异常：缺少 choices[0].message.content".to_string()
    })?;
    let text = coerce_chat_completion_message_text(content_raw)?;
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("视觉桥接返回空文本，请稍后重试或更换视觉模型".to_string());
    }
    if vision_should_check_degenerate_ocr_template(model) {
        reject_known_degenerate_ocr_output(&text)?;
    }

    Ok(text)
}

#[cfg(test)]
mod save_config_tests {
    use super::*;
    use deepseek_config::VisionConfigToml;
    use std::io::Write;
    use uuid::Uuid;

    fn temp_config_path() -> PathBuf {
        std::env::temp_dir().join(format!("ds-pick-cfg-test-{}.toml", Uuid::new_v4()))
    }

    #[test]
    fn merge_deepseek_key_preserves_vision_section() {
        let path = temp_config_path();
        let mut f = std::fs::File::create(&path).expect("create");
        f.write_all(
            br#"api_key = "old"
[vision]
api_key = "vk"
base_url = "https://api.siliconflow.cn/v1"
model = "deepseek-ai/DeepSeek-OCR"
"#,
        )
        .expect("write");

        let mut store = ConfigStore::load(Some(path.clone())).expect("load");
        merge_deepseek_api_key(&mut store.config, "new-key");
        store.save().expect("save");

        let parsed: ConfigToml =
            toml::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(parsed.api_key.as_deref(), Some("new-key"));
        let vision = parsed.vision.expect("vision");
        assert_eq!(vision.api_key.as_deref(), Some("vk"));
        assert_eq!(
            vision.base_url.as_deref(),
            Some("https://api.siliconflow.cn/v1")
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn save_vision_bridge_roundtrip() {
        let path = temp_config_path();
        let mut store = ConfigStore::load(Some(path.clone())).expect("load");
        store.config.vision = Some(VisionConfigToml {
            api_key: Some("vkey".to_string()),
            base_url: Some("https://x/v1".to_string()),
            model: Some("m".to_string()),
        });
        store.save().expect("save");

        let mut store = ConfigStore::load(Some(path.clone())).expect("reload");
        merge_deepseek_api_key(&mut store.config, "ds");
        store.save().expect("save2");

        let parsed: ConfigToml =
            toml::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(parsed.providers.deepseek.api_key.as_deref(), Some("ds"));
        let v = parsed.vision.expect("vision");
        assert_eq!(v.api_key.as_deref(), Some("vkey"));
        std::fs::remove_file(&path).ok();
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

// ---------------------------------------------------------------------------
// system settings — 桌面系统设置面板读写 config.toml（双轨同步）
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemSettings {
    pub default_model: String,
    pub reasoning_effort: String,
    pub cost_currency: String,
    pub allow_shell: bool,
    pub approval_policy: String,
    pub sandbox_mode: String,
    pub max_subagents: usize,
    pub web_search: bool,
    pub subagents_enabled: bool,
    pub exec_policy: bool,
    pub memory_enabled: bool,
    pub lsp_enabled: bool,
    pub snapshots_enabled: bool,
    pub notify_method: String,
    pub session_file_mb: u64,
}

#[tauri::command]
pub fn get_system_settings() -> Result<SystemSettings, String> {
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let cfg = &store.config;
    Ok(SystemSettings {
        default_model: cfg
            .default_text_model
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "deepseek-v4-pro".into()),
        reasoning_effort: cfg
            .reasoning_effort
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "max".into()),
        cost_currency: cfg
            .cost_currency
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "usd".into()),
        allow_shell: cfg.allow_shell.unwrap_or(false),
        approval_policy: cfg
            .approval_policy
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "on-request".into()),
        sandbox_mode: cfg
            .sandbox_mode
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "workspace-write".into()),
        // max_subagents: [subagents].max_concurrent > 顶层 max_subagents > 默认 10
        max_subagents: cfg
            .subagents
            .as_ref()
            .and_then(|s| s.max_concurrent)
            .or(cfg.max_subagents)
            .unwrap_or(10)
            .clamp(1, 20),
        web_search: cfg
            .features
            .as_ref()
            .and_then(|f| f.web_search)
            .unwrap_or(true),
        subagents_enabled: cfg
            .features
            .as_ref()
            .and_then(|f| f.subagents)
            .unwrap_or(true),
        exec_policy: cfg
            .features
            .as_ref()
            .and_then(|f| f.exec_policy)
            .unwrap_or(true),
        memory_enabled: cfg
            .memory
            .as_ref()
            .and_then(|m| m.enabled)
            .unwrap_or(false),
        lsp_enabled: cfg.lsp.as_ref().and_then(|l| l.enabled).unwrap_or(true),
        snapshots_enabled: cfg.snapshots.as_ref().map(|s| s.enabled).unwrap_or(true),
        notify_method: cfg
            .notifications
            .as_ref()
            .and_then(|n| n.method.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "auto".into()),
        session_file_mb: cfg.session.as_ref().map(|s| s.max_file_mb).unwrap_or(5),
    })
}

#[tauri::command]
pub fn save_system_settings(
    settings: SystemSettings,
    ctx: tauri::State<'_, AppContext>,
) -> Result<(), String> {
    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let cfg = &mut store.config;

    // 顶层标量字段
    cfg.default_text_model = Some(settings.default_model);
    cfg.reasoning_effort = Some(settings.reasoning_effort);
    cfg.cost_currency = Some(settings.cost_currency);
    cfg.allow_shell = Some(settings.allow_shell);
    cfg.approval_policy = Some(settings.approval_policy);
    cfg.sandbox_mode = Some(settings.sandbox_mode);
    cfg.max_subagents = Some(settings.max_subagents);

    // 清掉 [subagents].max_concurrent 避免与顶层 max_subagents 不一致
    //（TUI Config::max_subagents() 优先读 [subagents] 表）
    if let Some(ref mut s) = cfg.subagents {
        s.max_concurrent = None;
    }

    // features：使用 get_or_insert_with 而非 take() ——
    // 避免丢弃 config.toml 中已有的其他 features 字段
    let features = cfg.features.get_or_insert_with(Default::default);
    features.web_search = Some(settings.web_search);
    features.exec_policy = Some(settings.exec_policy);
    features.subagents = Some(settings.subagents_enabled);

    // memory
    let memory = cfg.memory.get_or_insert_with(Default::default);
    memory.enabled = Some(settings.memory_enabled);

    // lsp
    let lsp = cfg.lsp.get_or_insert_with(Default::default);
    lsp.enabled = Some(settings.lsp_enabled);

    // snapshots
    let snapshots = cfg.snapshots.get_or_insert_with(Default::default);
    snapshots.enabled = settings.snapshots_enabled;

    // notifications
    let notif = cfg.notifications.get_or_insert_with(Default::default);
    notif.method = Some(settings.notify_method);

    // session
    let session = cfg.session.get_or_insert_with(Default::default);
    session.max_file_mb = settings.session_file_mb;

    tracing::info!("save_system_settings: writing config");

    store.save().map_err(|e| e.to_string())?;

    // 重启 sidecar 使 TUI Config 重新读取 config.toml
    ctx.sidecar_restart.notify_one();
    Ok(())
}

#[tauri::command]
pub fn restart_sidecar(ctx: tauri::State<'_, AppContext>) -> Result<(), String> {
    ctx.sidecar_restart.notify_one();
    Ok(())
}

// ---------------------------------------------------------------------------
// pick-rules — `.deepseek/pick-rules.md` per workspace (DS Pick project rules)
// ---------------------------------------------------------------------------

/// Matches `crates/tui/src/prompts.rs` `INSTRUCTIONS_FILE_MAX_BYTES`.
const PICK_RULES_MAX_BYTES: usize = 100 * 1024;

fn workspace_root_canonical(raw: &str) -> Result<PathBuf, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err("工作区路径不能为空".to_string());
    }
    let p = PathBuf::from(t);
    let base = p
        .canonicalize()
        .map_err(|e| format!("工作区路径无效: {e}"))?;
    if !base.is_dir() {
        return Err("工作区必须是目录".to_string());
    }
    Ok(base)
}

fn pick_rules_path_under_workspace(base: &Path) -> PathBuf {
    base.join(".deepseek").join("pick-rules.md")
}

/// Read DS Pick project rules for a workspace. Returns empty string if the file is missing.
#[tauri::command]
pub fn read_pick_rules(workspace_root: String) -> Result<String, String> {
    let base = workspace_root_canonical(&workspace_root)?;
    let path = pick_rules_path_under_workspace(&base);
    if !path.is_file() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).map_err(|e| format!("读取项目规则失败: {e}"))
}

/// Write DS Pick project rules. Creates `.deepseek/` when needed.
#[tauri::command]
pub fn save_pick_rules(workspace_root: String, content: String) -> Result<(), String> {
    let base = workspace_root_canonical(&workspace_root)?;
    let deepseek = base.join(".deepseek");
    std::fs::create_dir_all(&deepseek).map_err(|e| format!("创建 .deepseek 目录失败: {e}"))?;
    let path = pick_rules_path_under_workspace(&base);

    if content.as_bytes().len() > PICK_RULES_MAX_BYTES {
        return Err(format!(
            "规则内容过长（最大 {} KiB，与 instructions 文件上限一致）",
            PICK_RULES_MAX_BYTES / 1024
        ));
    }

    std::fs::write(&path, content.as_str().as_bytes())
        .map_err(|e| format!("写入项目规则失败: {e}"))?;
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

// ---------------------------------------------------------------------------
// rebuild_symbol_index — trigger symbol index rebuild for a workspace
// ---------------------------------------------------------------------------

/// Calls the runtime's `POST /v1/symbol-index/rebuild` endpoint.
#[tauri::command]
pub async fn rebuild_symbol_index(
    ctx: tauri::State<'_, AppContext>,
    workspace: String,
) -> Result<(), String> {
    let runtime_port = ctx.runtime_port;
    let token = &ctx.runtime_token;
    let client = reqwest::Client::new();
    let url = format!(
        "http://127.0.0.1:{runtime_port}/v1/symbol-index/rebuild?workspace={}",
        urlencoding(&workspace)
    );
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("索引重建失败 ({status}): {body}"));
    }
    Ok(())
}

/// Percent-encode a string for use in URL query parameters.
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}