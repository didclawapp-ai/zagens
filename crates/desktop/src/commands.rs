use deepseek_config::{ConfigStore, ConfigToml};
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::sync::Arc;
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
    #[cfg(not(unix))]
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
