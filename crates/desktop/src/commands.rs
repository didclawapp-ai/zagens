//! Zagens desktop Tauri IPC commands (`#[tauri::command]`).
//!
//! **D1 / 架构定型：** 本文件保持单体（~1.5k 行）。命令集中便于检索；避免拆成过多小模块。
//! 后续按域（auth / vision / settings / terminal 等）**按需**再拆。见
//! [`ARCHITECTURE_ASSESSMENT_2026-05-25.md`](../../../docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md) §5.1「D1 — 已闭合」。

use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use zagens_config::{
    CompactionToml, CompletionGateConfigToml, ConfigStore, ConfigToml, DEFAULT_VISION_MODEL,
    HookConditionToml, HookEventToml, HookToml, HooksConfigToml, LhtPresetId,
    LongHorizonConfigToml, MacroLoopConfigToml, WORKSPACE_META_DIR_NAME, WindowsConfigToml,
    WindowsSandboxModeToml, apply_lht_preset as apply_lht_preset_overlay,
    compaction_threshold_tokens_for_model, legacy_workspace_meta_dir, lht_product_defaults,
    normalize_gate_mode, normalize_lht_mode, resolve_lht,
    vision_should_check_degenerate_ocr_template, vision_user_prompt_for_model, workspace_meta_dir,
    workspace_meta_dir_read, workspace_meta_file_read, workspace_meta_file_write,
};

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
    /// Dynamic runtime port channel. Initial value is `0` until the sidecar prints
    /// `DS_PICK_READY {port: N}` to stdout (parsed by [`crate::sidecar::spawn_stdout_forwarder`]),
    /// after which the supervisor publishes the real bound port through this watch channel.
    /// Use [`AppContext::require_port`] from IPC handlers; use `runtime_port.changed().await`
    /// to wait for the first publish (see [`get_runtime_port`]).
    pub runtime_port: tokio::sync::watch::Receiver<u16>,
    pub runtime_token: String,
    /// Wake the sidecar supervisor to restart `deepseek-runtime`'s HTTP server (reload `config.toml`).
    pub sidecar_restart: Arc<Notify>,
    /// Signal the sidecar supervisor to shut down (kill the child process and exit).
    pub shutdown: Arc<Notify>,
}

impl AppContext {
    /// Current published runtime port (`0` before sidecar `DS_PICK_READY`).
    pub fn current_port(&self) -> u16 {
        *self.runtime_port.borrow()
    }

    /// Returns the current port or a user-facing error if the sidecar is not yet ready.
    /// IPC handlers should `?`-propagate this; web-ui should call `get_runtime_port` first
    /// (which awaits the first publish) before invoking other runtime-touching commands.
    pub fn require_port(&self) -> Result<u16, String> {
        let p = self.current_port();
        if p == 0 {
            Err("runtime sidecar 尚未就绪（端口未发布）".to_string())
        } else {
            Ok(p)
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub version: String,
}

/// Windows native sandbox posture for Settings UI (Phase 3 / PR-3.4).
#[derive(Debug, Serialize)]
pub struct WindowsSandboxStatus {
    pub setup_complete: bool,
    pub enforced: bool,
    /// `elevated` | `unelevated` | `none`
    pub effective_mode: String,
}

#[tauri::command]
pub fn get_windows_sandbox_status() -> Result<WindowsSandboxStatus, String> {
    #[cfg(windows)]
    {
        let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
        let home = zagens_windows_sandbox::zagens_home();
        let setup_complete = zagens_windows_sandbox::sandbox_setup_is_complete(&home);
        let unelevated_ready = zagens_windows_sandbox::is_enforcement_available();
        let configured = windows_configured_sandbox_label(&store.config);
        let effective_mode =
            windows_effective_sandbox_backend(configured, setup_complete, unelevated_ready);
        Ok(WindowsSandboxStatus {
            setup_complete,
            enforced: effective_mode != "none",
            effective_mode,
        })
    }
    #[cfg(not(windows))]
    {
        Err("Windows sandbox status is only available on Windows".to_string())
    }
}

/// Per-platform sandbox posture for the Sandbox settings panel (cross-host overview).
#[derive(Debug, Serialize)]
pub struct SandboxPlatformStatus {
    pub enforced: bool,
    pub backend_available: bool,
    /// Effective runtime backend (`elevated` | `unelevated` | `none`, etc.).
    pub backend: String,
    /// Config file selection (`auto` | `elevated` | `unelevated` on Windows).
    pub configured_backend: String,
    pub setup_complete: Option<bool>,
    /// Windows first-run wizard completed (host-only).
    pub sandbox_initialized: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct SandboxPlatformsOverview {
    pub host_os: String,
    pub windows: SandboxPlatformStatus,
    pub linux: SandboxPlatformStatus,
    pub macos: SandboxPlatformStatus,
}

#[derive(Debug, Serialize)]
pub struct SandboxOnboardingState {
    /// User completed first-run sandbox onboarding on this host.
    pub initialized: bool,
    /// When true, Settings UI shows the onboarding wizard instead of full controls.
    pub show_wizard: bool,
}

fn windows_configured_sandbox_label(cfg: &ConfigToml) -> &'static str {
    match cfg.windows.as_ref().and_then(|w| w.sandbox) {
        Some(WindowsSandboxModeToml::Elevated) => "elevated",
        Some(WindowsSandboxModeToml::Unelevated) => "unelevated",
        None => "auto",
    }
}

fn windows_sandbox_onboarding_initialized(cfg: &ConfigToml) -> bool {
    if cfg.windows.as_ref().and_then(|w| w.sandbox_initialized) == Some(true) {
        return true;
    }
    #[cfg(windows)]
    {
        let home = zagens_windows_sandbox::zagens_home();
        if zagens_windows_sandbox::sandbox_setup_is_complete(&home) {
            return true;
        }
    }
    cfg.windows.as_ref().and_then(|w| w.sandbox).is_some()
}

#[cfg(windows)]
fn windows_effective_sandbox_backend(
    configured: &str,
    setup_complete: bool,
    unelevated_ready: bool,
) -> String {
    match configured {
        "unelevated" => {
            if unelevated_ready {
                "unelevated".into()
            } else {
                "none".into()
            }
        }
        "elevated" => {
            if setup_complete {
                "elevated".into()
            } else if unelevated_ready {
                "unelevated".into()
            } else {
                "none".into()
            }
        }
        _ => {
            if setup_complete {
                "elevated".into()
            } else if unelevated_ready {
                "unelevated".into()
            } else {
                "none".into()
            }
        }
    }
}

#[cfg(windows)]
fn format_windows_setup_error(failure: &zagens_windows_sandbox::SetupFailure) -> String {
    if failure.code == zagens_windows_sandbox::SetupErrorCode::OrchestratorHelperLaunchCanceled {
        "Windows sandbox setup was canceled (UAC prompt dismissed).".into()
    } else {
        format!(
            "Windows sandbox setup failed ({}): {}",
            failure.code.as_str(),
            failure.message
        )
    }
}

#[cfg(windows)]
fn map_windows_setup_error(err: anyhow::Error) -> String {
    if let Some(failure) = zagens_windows_sandbox::extract_setup_failure(&err) {
        format_windows_setup_error(failure)
    } else {
        err.to_string()
    }
}

fn linux_landlock_probe() -> bool {
    #[cfg(target_os = "linux")]
    {
        const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1;
        // Safety: null ruleset pointer is ABI version probe only.
        unsafe {
            let result = libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<libc::c_void>(),
                0usize,
                LANDLOCK_CREATE_RULESET_VERSION,
            );
            result >= 0
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

fn macos_seatbelt_probe() -> bool {
    #[cfg(target_os = "macos")]
    {
        Path::new("/usr/bin/sandbox-exec").exists()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[tauri::command]
pub fn get_sandbox_platforms_overview() -> Result<SandboxPlatformsOverview, String> {
    let host_os = std::env::consts::OS.to_string();
    let config = ConfigStore::load(None).map(|store| store.config).ok();

    let windows = {
        #[cfg(windows)]
        {
            let home = zagens_windows_sandbox::zagens_home();
            let setup_complete = zagens_windows_sandbox::sandbox_setup_is_complete(&home);
            let unelevated_ready = zagens_windows_sandbox::is_enforcement_available();
            let configured_backend = config
                .as_ref()
                .map(windows_configured_sandbox_label)
                .unwrap_or("auto")
                .to_string();
            let backend = windows_effective_sandbox_backend(
                configured_backend.as_str(),
                setup_complete,
                unelevated_ready,
            );
            let initialized = config
                .as_ref()
                .map(windows_sandbox_onboarding_initialized)
                .unwrap_or(false);
            SandboxPlatformStatus {
                enforced: backend != "none",
                backend_available: setup_complete || unelevated_ready,
                backend,
                configured_backend,
                setup_complete: Some(setup_complete),
                sandbox_initialized: Some(initialized),
            }
        }
        #[cfg(not(windows))]
        {
            SandboxPlatformStatus {
                enforced: false,
                backend_available: false,
                backend: "none".into(),
                configured_backend: "n/a".into(),
                setup_complete: None,
                sandbox_initialized: None,
            }
        }
    };

    let landlock = linux_landlock_probe();
    let linux = SandboxPlatformStatus {
        enforced: false,
        backend_available: landlock,
        backend: if landlock {
            "landlock".into()
        } else {
            "none".into()
        },
        configured_backend: "n/a".into(),
        setup_complete: None,
        sandbox_initialized: None,
    };

    let seatbelt = macos_seatbelt_probe();
    let macos = SandboxPlatformStatus {
        enforced: seatbelt,
        backend_available: seatbelt,
        backend: if seatbelt {
            "seatbelt".into()
        } else {
            "none".into()
        },
        configured_backend: "n/a".into(),
        setup_complete: None,
        sandbox_initialized: None,
    };

    Ok(SandboxPlatformsOverview {
        host_os,
        windows,
        linux,
        macos,
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SandboxSettings {
    pub sandbox_mode: String,
    /// `auto` | `elevated` | `unelevated` — `auto` clears explicit `[windows] sandbox`.
    pub windows_sandbox: String,
    pub windows_private_desktop: bool,
}

fn sandbox_settings_from_config(cfg: &ConfigToml) -> SandboxSettings {
    let windows_sandbox = cfg
        .windows
        .as_ref()
        .and_then(|w| w.sandbox)
        .map(|m| match m {
            WindowsSandboxModeToml::Elevated => "elevated",
            WindowsSandboxModeToml::Unelevated => "unelevated",
        })
        .unwrap_or("auto")
        .to_string();
    SandboxSettings {
        sandbox_mode: cfg
            .sandbox_mode
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "workspace-write".into()),
        windows_sandbox,
        windows_private_desktop: cfg
            .windows
            .as_ref()
            .and_then(|w| w.sandbox_private_desktop)
            .unwrap_or(false),
    }
}

#[tauri::command]
pub fn get_sandbox_onboarding_state() -> Result<SandboxOnboardingState, String> {
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let host_os = std::env::consts::OS;
    let initialized = if host_os == "windows" {
        windows_sandbox_onboarding_initialized(&store.config)
    } else {
        true
    };
    Ok(SandboxOnboardingState {
        initialized,
        show_wizard: host_os == "windows" && !initialized,
    })
}

#[tauri::command]
pub async fn initialize_windows_sandbox(
    mode: String,
    ctx: tauri::State<'_, AppContext>,
) -> Result<SandboxSettings, String> {
    #[cfg(not(windows))]
    {
        let _ = (mode, ctx);
        return Err("Windows sandbox initialization is only available on Windows".into());
    }

    #[cfg(windows)]
    {
        let mode = mode.trim().to_ascii_lowercase();
        if mode == "elevated" {
            let real_user = std::env::var("USERNAME").map_err(|e| e.to_string())?;
            let setup_result = tokio::task::spawn_blocking(move || {
                zagens_windows_sandbox::run_elevated_provisioning_setup_default(&real_user)
            })
            .await
            .map_err(|e| format!("Windows sandbox setup task failed: {e}"))?;
            setup_result.map_err(map_windows_setup_error)?;

            let home = zagens_windows_sandbox::zagens_home();
            if !zagens_windows_sandbox::sandbox_setup_is_complete(&home) {
                return Err(
                    "Windows sandbox setup finished but setup marker is still missing.".into(),
                );
            }
        } else if mode != "unelevated" {
            return Err(format!(
                "Invalid initialize mode '{mode}': expected elevated or unelevated."
            ));
        }

        let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
        let cfg = &mut store.config;
        cfg.sandbox_mode = Some("workspace-write".into());
        let windows = cfg.windows.get_or_insert_with(WindowsConfigToml::default);
        windows.sandbox = Some(if mode == "elevated" {
            WindowsSandboxModeToml::Elevated
        } else {
            WindowsSandboxModeToml::Unelevated
        });
        windows.sandbox_initialized = Some(true);
        windows.sandbox_private_desktop = Some(true);

        tracing::info!(target: "sandbox", mode = %mode, "initialize_windows_sandbox: writing config");
        store.save().map_err(|e| e.to_string())?;
        ctx.sidecar_restart.notify_one();
        Ok(sandbox_settings_from_config(&store.config))
    }
}

#[tauri::command]
pub fn get_sandbox_settings() -> Result<SandboxSettings, String> {
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    Ok(sandbox_settings_from_config(&store.config))
}

#[tauri::command]
pub fn save_sandbox_settings(
    settings: SandboxSettings,
    ctx: tauri::State<'_, AppContext>,
) -> Result<(), String> {
    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let cfg = &mut store.config;

    cfg.sandbox_mode = Some(settings.sandbox_mode);

    let windows = cfg.windows.get_or_insert_with(WindowsConfigToml::default);
    windows.sandbox = match settings
        .windows_sandbox
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "auto" | "" => None,
        "elevated" => Some(WindowsSandboxModeToml::Elevated),
        "unelevated" => Some(WindowsSandboxModeToml::Unelevated),
        other => {
            return Err(format!(
                "Invalid windows_sandbox '{other}': expected auto, elevated, or unelevated."
            ));
        }
    };
    windows.sandbox_private_desktop = Some(settings.windows_private_desktop);
    #[cfg(windows)]
    {
        windows.sandbox_initialized = Some(true);
    }

    tracing::info!("save_sandbox_settings: writing config");
    store.save().map_err(|e| e.to_string())?;
    ctx.sidecar_restart.notify_one();
    Ok(())
}

#[tauri::command]
pub async fn get_runtime_port(ctx: tauri::State<'_, AppContext>) -> Result<u16, String> {
    // Block the IPC call (not the runtime) until the supervisor publishes the real port.
    // The web-ui calls this once in `initRuntimeConfig`; spinning here keeps the JS side
    // simple (no retry loop) and naturally serializes the rest of `client.ts` behind a ready sidecar.
    let mut rx = ctx.runtime_port.clone();
    loop {
        let port = *rx.borrow();
        if port != 0 {
            return Ok(port);
        }
        rx.changed()
            .await
            .map_err(|e| format!("runtime port watch channel closed: {e}"))?;
    }
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
    let secrets = zagens_secrets::Secrets::auto_detect();
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
    let secrets = zagens_secrets::Secrets::auto_detect();
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

#[tauri::command]
pub fn clear_deepseek_api_key(ctx: tauri::State<'_, AppContext>) -> Result<(), String> {
    let secrets = zagens_secrets::Secrets::auto_detect();
    secrets
        .delete("deepseek")
        .map_err(|e| format!("无法从系统密钥链删除: {e}"))?;

    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    store.config.api_key = None;
    store.config.providers.deepseek.api_key = None;
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
    let secrets = zagens_secrets::Secrets::auto_detect();
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
        let secrets = zagens_secrets::Secrets::auto_detect();
        if secrets.resolve("vision").is_none() {
            return Err(
                "请填写视觉桥接 API Key；密钥保存后不会回显。修改端点或模型时也需要重新输入密钥。"
                    .to_string(),
            );
        }
    } else {
        let secrets = zagens_secrets::Secrets::auto_detect();
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
    v.model = if m.is_empty() {
        None
    } else {
        Some(m.to_string())
    };

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
    let secrets = zagens_secrets::Secrets::auto_detect();
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
        if let Some(t) = item.get("text").and_then(|v| v.as_str())
            && !t.trim().is_empty()
        {
            out.push(t.to_string());
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
    let vision = store
        .config
        .vision
        .as_ref()
        .ok_or_else(|| "未配置视觉桥接：请在 设置 → API Key 中保存视觉桥接密钥".to_string())?;
    // Key from OS keyring first, then config.toml fallback (legacy plaintext)
    let secrets = zagens_secrets::Secrets::auto_detect();
    let api_key = secrets
        .resolve("vision")
        .or_else(|| {
            vision
                .api_key
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .cloned()
        })
        .ok_or_else(|| "未配置视觉桥接 API Key".to_string())?;
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
    let upload_est_seconds =
        |mbps: f64| -> f64 { (body_bytes.len() as f64 * 8.0) / (mbps * 1_000_000.0) };
    eprintln!(
        "[vision] POST {}/chat/completions  body={:.1} MB  model={}  timeout={}s \
         上传耗时估算: 10Mbps≈{:.1}s 5Mbps≈{:.1}s 2Mbps≈{:.1}s 1Mbps≈{:.1}s",
        base_url,
        body_mb,
        model,
        timeout_secs,
        upload_est_seconds(10.0),
        upload_est_seconds(5.0),
        upload_est_seconds(2.0),
        upload_est_seconds(1.0),
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

    let content_raw = resp_body["choices"]
        .get(0)
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .ok_or_else(|| "视觉桥接响应格式异常：缺少 choices[0].message.content".to_string())?;
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
    use std::io::Write;
    use uuid::Uuid;
    use zagens_config::VisionConfigToml;

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
    zagens_config::read_locale_setting().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_app_locale(locale: String) -> Result<(), String> {
    zagens_config::write_locale_setting(&locale).map_err(|e| e.to_string())
}

/// Read the composer LHT tri-state (`auto` | `strict` | `off`).
#[tauri::command]
pub async fn get_lht_composer_mode() -> Result<String, String> {
    Ok(zagens_config::read_lht_composer_mode_setting()
        .map_err(|e| e.to_string())?
        .as_str()
        .to_string())
}

/// Persist the composer LHT tri-state. Takes effect on the next turn without restart.
#[tauri::command]
pub fn set_lht_composer_mode(mode: String) -> Result<(), String> {
    zagens_config::write_lht_composer_mode_setting(zagens_config::LhtComposerMode::from_storage(
        &mode,
    ))
    .map_err(|e| e.to_string())
}

/// Legacy: read strict flag (`true` only when mode is strict).
#[tauri::command]
pub async fn get_lht_strict() -> Result<bool, String> {
    zagens_config::read_lht_strict_setting().map_err(|e| e.to_string())
}

/// Legacy: `true` → strict; `false` → auto (not off).
#[tauri::command]
pub fn set_lht_strict(enabled: bool) -> Result<(), String> {
    zagens_config::write_lht_strict_setting(enabled).map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct DesktopShellPrefs {
    pub onboarding_complete: bool,
    pub task_type_preference: String,
}

/// Read desktop shell prefs persisted in `settings.toml` (survives WebView storage resets).
#[tauri::command]
pub fn get_desktop_shell_prefs() -> Result<DesktopShellPrefs, String> {
    let onboarding_complete =
        zagens_config::read_onboarding_complete_setting().map_err(|e| e.to_string())?;
    let task_type_preference = zagens_config::read_task_type_preference_setting()
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "auto".to_string());
    Ok(DesktopShellPrefs {
        onboarding_complete,
        task_type_preference,
    })
}

/// Persist onboarding completion and default task type to `settings.toml`.
#[tauri::command]
pub fn save_desktop_shell_prefs(
    onboarding_complete: bool,
    task_type_preference: String,
) -> Result<(), String> {
    zagens_config::write_task_type_preference_setting(&task_type_preference)
        .map_err(|e| e.to_string())?;
    zagens_config::write_onboarding_complete_setting(onboarding_complete).map_err(|e| e.to_string())
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
    use std::io::Read;

    let meta = std::fs::metadata(canonical_file).map_err(|e| format!("无法获取文件信息: {e}"))?;
    let size = meta.len();
    let truncated = size > PREVIEW_MAX_BINARY_BYTES;
    let read_limit = if truncated {
        PREVIEW_MAX_BINARY_BYTES as usize
    } else {
        size as usize
    };

    let mut file = std::fs::File::open(canonical_file).map_err(|e| format!("无法读取文件: {e}"))?;
    let mut data = Vec::with_capacity(read_limit);
    file.by_ref()
        .take(read_limit as u64)
        .read_to_end(&mut data)
        .map_err(|e| format!("无法读取文件: {e}"))?;

    let mime_type = sniff_mime(&data);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);

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
        fetch_thread_workspace_root(ctx.require_port()?, &ctx.runtime_token, &thread_id).await?;
    let path = resolve_under_workspace(&root, &relative_path)?;
    read_binary_file_at(&path)
}

#[tauri::command]
pub fn read_workspace_binary_at_root(
    workspace_root: String,
    relative_path: String,
) -> Result<BinaryFileResponse, String> {
    let root = user_scoped_workspace_root(&workspace_root)?;
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
    let canonical = resolve_shell_dir_path(&path)?;
    let open_path = canonical.to_string_lossy().into_owned();
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&open_path)
            .spawn()
            .map_err(|e| format!("无法打开文件管理器: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&open_path)
            .spawn()
            .map_err(|e| format!("无法打开文件管理器: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&open_path)
            .spawn()
            .map_err(|e| format!("无法打开文件管理器: {e}"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// export_thread_json — fetches thread from runtime and writes to a file
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// open_with_system_app — opens a file with the system default application
// ---------------------------------------------------------------------------

/// File extensions that should be opened with the system app rather than
/// the built-in text viewer.  Matched case-insensitively.
fn is_system_openable(ext_lower: &str) -> bool {
    matches!(
        ext_lower,
        "pdf"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "svg"
            | "webp"
            | "bmp"
            | "ico"
            | "xlsx"
            | "xls"
            | "docx"
            | "doc"
            | "pptx"
            | "ppt"
            | "zip"
            | "rar"
            | "7z"
            | "tar"
            | "gz"
    )
}

fn resolve_system_open_path(path: &str) -> Result<std::path::PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("empty path".into());
    }
    reject_unsafe_open_path_chars(trimmed)?;
    let canonical = std::path::Path::new(trimmed)
        .canonicalize()
        .map_err(|e| format!("path not found: {e}"))?;
    if !canonical.is_file() {
        return Err("not a file".into());
    }
    Ok(canonical)
}

fn reject_unsafe_open_path_chars(trimmed: &str) -> Result<(), String> {
    if trimmed.bytes().any(|b| b == 0) {
        return Err("invalid path".into());
    }
    const BAD: &[char] = &['&', '|', '<', '>', '^', '%', '\n', '\r'];
    if trimmed.chars().any(|c| BAD.contains(&c)) {
        return Err("path contains invalid characters".into());
    }
    Ok(())
}

fn resolve_shell_dir_path(path: &str) -> Result<std::path::PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("empty path".into());
    }
    reject_unsafe_open_path_chars(trimmed)?;
    let canonical = std::path::Path::new(trimmed)
        .canonicalize()
        .map_err(|e| format!("path not found: {e}"))?;
    if !canonical.is_dir() {
        return Err("not a directory".into());
    }
    Ok(canonical)
}

#[tauri::command]
pub fn open_with_system_app(path: String) -> Result<(), String> {
    let canonical = resolve_system_open_path(&path)?;
    let ext = canonical
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if !is_system_openable(&ext) {
        return Err(format!(
            "不支持用系统应用打开 .{ext} 文件；仅支持 PDF、图片、Office 文档等格式"
        ));
    }

    open::that(&canonical).map_err(|e| format!("无法打开文件: {e}"))
}

/// Open an http(s) or mailto URL in the system default handler (browser / mail client).
#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("链接为空".into());
    }
    let allowed =
        url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:");
    if !allowed {
        return Err("仅支持 http(s) 与 mailto 链接".into());
    }
    open::that(url).map_err(|e| format!("无法打开链接: {e}"))
}

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
    let url = format!(
        "http://127.0.0.1:{}/v1/threads/{}",
        ctx.require_port()?,
        enc
    );
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

    let out_path = crate::export_path::validate_export_json_path(&save_path)?;
    std::fs::write(&out_path, json).map_err(|e| format!("保存失败: {e}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// system settings — 桌面系统设置面板读写 config.toml（双轨同步）
// ---------------------------------------------------------------------------

fn subagent_extra_string(
    subagents: Option<&zagens_config::SubagentsConfigToml>,
    key: &str,
) -> String {
    subagents
        .and_then(|s| s.extras.get(key))
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn set_subagent_extra_string(
    subagents: &mut zagens_config::SubagentsConfigToml,
    key: &str,
    value: &str,
) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        subagents.extras.remove(key);
    } else {
        subagents
            .extras
            .insert(key.to_string(), toml::Value::String(trimmed.to_string()));
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemSettings {
    pub default_model: String,
    pub reasoning_effort: String,
    pub cost_currency: String,
    pub allow_shell: bool,
    pub approval_policy: String,
    pub sandbox_mode: String,
    pub max_subagents: usize,
    /// Per-step sub-agent LLM API timeout (seconds), from `[subagents] step_timeout_secs`.
    pub subagent_step_timeout_secs: u64,
    /// CRAFT role model overrides (`[subagents]` table, empty = inherit parent model).
    #[serde(default)]
    pub subagent_review_model: String,
    #[serde(default)]
    pub subagent_implementer_model: String,
    #[serde(default)]
    pub subagent_verifier_model: String,
    #[serde(default)]
    pub subagent_auditor_model: String,
    pub web_search: bool,
    pub subagents_enabled: bool,
    pub exec_policy: bool,
    pub memory_enabled: bool,
    /// Topic memory graph injection (`[topic_memory]`, B2).
    pub topic_memory_enabled: bool,
    /// Inject cognitive map every N completed turns (default 5).
    pub topic_memory_inject_interval: u32,
    pub lsp_enabled: bool,
    pub snapshots_enabled: bool,
    pub notify_method: String,
    pub session_file_mb: u64,
    /// Enable automatic context compaction (TUI `auto_compact` / engine policy).
    pub auto_compact: bool,
    /// Token threshold for auto-compaction (TUI `CompactionConfig.token_threshold`).
    pub compaction_threshold_tokens: usize,
    /// Model-derived default threshold (80% window) for UI hints — read-only on save.
    pub compaction_threshold_default: usize,
    /// Distinct model ids from config.toml (default + per-provider); read-only on save.
    #[serde(default)]
    pub available_models: Vec<String>,
}

fn push_model_option(
    out: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
    raw: Option<&str>,
) {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };
    let key = raw.to_ascii_lowercase();
    if seen.insert(key) {
        out.push(raw.to_string());
    }
}

fn collect_configured_models(cfg: &zagens_config::ConfigToml) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    push_model_option(&mut out, &mut seen, cfg.default_text_model.as_deref());
    push_model_option(&mut out, &mut seen, cfg.model.as_deref());
    let providers = &cfg.providers;
    push_model_option(&mut out, &mut seen, providers.deepseek.model.as_deref());
    push_model_option(&mut out, &mut seen, providers.nvidia_nim.model.as_deref());
    push_model_option(&mut out, &mut seen, providers.openai.model.as_deref());
    push_model_option(&mut out, &mut seen, providers.openrouter.model.as_deref());
    push_model_option(&mut out, &mut seen, providers.novita.model.as_deref());
    push_model_option(&mut out, &mut seen, providers.fireworks.model.as_deref());
    push_model_option(&mut out, &mut seen, providers.sglang.model.as_deref());
    push_model_option(&mut out, &mut seen, providers.vllm.model.as_deref());
    push_model_option(&mut out, &mut seen, providers.ollama.model.as_deref());
    out
}

#[tauri::command]
pub fn get_system_settings() -> Result<SystemSettings, String> {
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let cfg = &store.config;
    let default_model = cfg
        .default_text_model
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "deepseek-v4-pro".into());
    let threshold_default = compaction_threshold_tokens_for_model(&default_model);
    Ok(SystemSettings {
        default_model,
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
        subagent_step_timeout_secs: cfg
            .subagents
            .as_ref()
            .and_then(|s| s.step_timeout_secs)
            .unwrap_or(600)
            .clamp(120, 1800),
        subagent_review_model: subagent_extra_string(cfg.subagents.as_ref(), "review_model"),
        subagent_implementer_model: subagent_extra_string(
            cfg.subagents.as_ref(),
            "implementer_model",
        ),
        subagent_verifier_model: subagent_extra_string(cfg.subagents.as_ref(), "verifier_model"),
        subagent_auditor_model: subagent_extra_string(cfg.subagents.as_ref(), "auditor_model"),
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
        memory_enabled: cfg.memory.as_ref().and_then(|m| m.enabled).unwrap_or(false),
        topic_memory_enabled: cfg
            .topic_memory
            .as_ref()
            .and_then(|t| t.enabled)
            .unwrap_or(false),
        topic_memory_inject_interval: cfg
            .topic_memory
            .as_ref()
            .and_then(|t| t.inject_interval)
            .unwrap_or(5)
            .max(1),
        lsp_enabled: cfg.lsp.as_ref().and_then(|l| l.enabled).unwrap_or(true),
        snapshots_enabled: cfg.snapshots.as_ref().map(|s| s.enabled).unwrap_or(true),
        notify_method: cfg
            .notifications
            .as_ref()
            .and_then(|n| n.method.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "auto".into()),
        session_file_mb: cfg.session.as_ref().map(|s| s.max_file_mb).unwrap_or(5),
        auto_compact: cfg
            .compaction
            .as_ref()
            .and_then(|c| c.auto_compact)
            .unwrap_or(false),
        compaction_threshold_tokens: cfg
            .compaction
            .as_ref()
            .and_then(|c| c.token_threshold)
            .unwrap_or(threshold_default),
        compaction_threshold_default: threshold_default,
        available_models: collect_configured_models(cfg),
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
    let subagents = cfg.subagents.get_or_insert_with(Default::default);
    subagents.step_timeout_secs = Some(settings.subagent_step_timeout_secs.clamp(120, 1800));
    set_subagent_extra_string(subagents, "review_model", &settings.subagent_review_model);
    set_subagent_extra_string(
        subagents,
        "implementer_model",
        &settings.subagent_implementer_model,
    );
    set_subagent_extra_string(
        subagents,
        "verifier_model",
        &settings.subagent_verifier_model,
    );
    set_subagent_extra_string(subagents, "auditor_model", &settings.subagent_auditor_model);

    // features：使用 get_or_insert_with 而非 take() ——
    // 避免丢弃 config.toml 中已有的其他 features 字段
    let features = cfg.features.get_or_insert_with(Default::default);
    features.web_search = Some(settings.web_search);
    features.exec_policy = Some(settings.exec_policy);
    features.subagents = Some(settings.subagents_enabled);

    // memory
    let memory = cfg.memory.get_or_insert_with(Default::default);
    memory.enabled = Some(settings.memory_enabled);

    // topic memory (B2)
    let topic_memory = cfg.topic_memory.get_or_insert_with(Default::default);
    topic_memory.enabled = Some(settings.topic_memory_enabled);
    topic_memory.inject_interval = Some(settings.topic_memory_inject_interval.max(1));

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

    // compaction — shared with TUI engine via config.toml `[compaction]`
    let compaction = cfg.compaction.get_or_insert_with(CompactionToml::default);
    compaction.auto_compact = Some(settings.auto_compact);
    compaction.token_threshold = Some(settings.compaction_threshold_tokens);

    tracing::info!("save_system_settings: writing config");

    store.save().map_err(|e| e.to_string())?;

    // 重启 sidecar 使 TUI Config 重新读取 config.toml
    ctx.sidecar_restart.notify_one();
    Ok(())
}

// ---------------------------------------------------------------------------
// LHT settings — desktop panel for `[long_horizon]` / `[long_horizon.completion_gate]`
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct LhtSettings {
    pub enabled: bool,
    /// `"auto"` | `"strict"`.
    pub mode: String,
    pub progress_via_git: bool,
    pub max_nudges_per_item: u32,
    pub blocked_nudges_without_progress: u32,
    pub auto_continue: bool,
    pub max_auto_continue_rounds: u32,
    /// `off` | `observe` | `enforce`.
    pub auto_verify_replay: String,
    pub toolchain_gate: String,
    pub stub_gate: String,
    pub max_manifest_rounds: u32,
    pub max_audit_rounds: u32,
    pub max_infra_strikes: u32,
    /// Read-only: operator `[[verify]]` rows preserved on save.
    pub custom_verify_count: u32,
    /// Read-only: operator `[[deliverable]]` rows preserved on save.
    pub custom_deliverable_count: u32,
    /// Phase 4: LHT↔CRAFT macro review loop (`[long_horizon.macro_loop]`).
    pub macro_loop_enabled: bool,
    pub macro_loop_max_cycles: u32,
    pub macro_loop_max_craft_rounds: u32,
    /// `user_confirm` | `on_micro_pass` | `off`
    pub macro_loop_auto_enter_craft: String,
    pub macro_loop_craft_on_small_tasks: bool,
    pub macro_loop_min_checklist_items: u32,
}

fn gate_mode_from_toml(raw: Option<&String>) -> String {
    normalize_gate_mode(raw.map(String::as_str).unwrap_or("off"))
}

fn lht_settings_from_config(cfg: &ConfigToml) -> LhtSettings {
    let lh = resolve_lht(&cfg.long_horizon);
    let gate = lh.completion_gate.clone().unwrap_or_default();
    LhtSettings {
        enabled: lh.enabled.unwrap_or(false),
        mode: normalize_lht_mode(lh.mode.as_deref().unwrap_or("auto")),
        progress_via_git: lh.progress_via_git.unwrap_or(true),
        max_nudges_per_item: lh.max_nudges_per_item.unwrap_or(5),
        blocked_nudges_without_progress: lh.blocked_nudges_without_progress.unwrap_or(3),
        auto_continue: lh.auto_continue.unwrap_or(false),
        max_auto_continue_rounds: lh.max_auto_continue_rounds.unwrap_or(16),
        auto_verify_replay: gate_mode_from_toml(gate.auto_verify_replay.as_ref()),
        toolchain_gate: gate_mode_from_toml(gate.toolchain_gate.as_ref()),
        stub_gate: normalize_gate_mode(gate.stub_gate.as_deref().unwrap_or("observe")),
        max_manifest_rounds: gate.max_manifest_rounds.unwrap_or(5),
        max_audit_rounds: gate.max_audit_rounds.unwrap_or(5),
        max_infra_strikes: gate.max_infra_strikes.unwrap_or(3),
        custom_verify_count: gate.verify.len() as u32,
        custom_deliverable_count: gate.deliverable.len() as u32,
        macro_loop_enabled: lh
            .macro_loop
            .as_ref()
            .and_then(|m| m.enabled)
            .unwrap_or(false),
        macro_loop_max_cycles: lh
            .macro_loop
            .as_ref()
            .and_then(|m| m.max_macro_cycles)
            .unwrap_or(3)
            .clamp(1, 8),
        macro_loop_max_craft_rounds: lh
            .macro_loop
            .as_ref()
            .and_then(|m| m.max_craft_rounds_per_cycle)
            .unwrap_or(2)
            .clamp(1, 4),
        macro_loop_auto_enter_craft: lh
            .macro_loop
            .as_ref()
            .and_then(|m| m.auto_enter_craft.as_deref())
            .map(normalize_macro_auto_enter)
            .unwrap_or_else(|| "user_confirm".into()),
        macro_loop_craft_on_small_tasks: lh
            .macro_loop
            .as_ref()
            .and_then(|m| m.craft_on_small_tasks)
            .unwrap_or(false),
        macro_loop_min_checklist_items: lh
            .macro_loop
            .as_ref()
            .and_then(|m| m.min_checklist_items_for_craft)
            .unwrap_or(3)
            .max(1),
    }
}

fn normalize_macro_auto_enter(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "on_micro_pass" | "auto" | "immediate" => "on_micro_pass".into(),
        "on_graph_complete" | "graph_complete" => "on_graph_complete".into(),
        "on_manifest_exhausted" | "manifest_exhausted" => "on_manifest_exhausted".into(),
        "off" | "disabled" | "false" => "off".into(),
        _ => "user_confirm".into(),
    }
}

#[tauri::command]
pub fn apply_lht_preset(
    preset_id: String,
    ctx: tauri::State<'_, AppContext>,
) -> Result<LhtSettings, String> {
    let preset = LhtPresetId::from_str_id(&preset_id)
        .ok_or_else(|| format!("unknown LHT preset: {preset_id}"))?;
    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let mut lh = store
        .config
        .long_horizon
        .take()
        .unwrap_or_else(lht_product_defaults);
    apply_lht_preset_overlay(&mut lh, preset);
    store.config.long_horizon = Some(lh);
    store.save().map_err(|e| e.to_string())?;
    ctx.sidecar_restart.notify_one();
    get_lht_settings()
}

#[tauri::command]
pub fn get_lht_settings() -> Result<LhtSettings, String> {
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    Ok(lht_settings_from_config(&store.config))
}

#[tauri::command]
pub fn save_lht_settings(
    settings: LhtSettings,
    ctx: tauri::State<'_, AppContext>,
) -> Result<(), String> {
    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let existing_gate = store
        .config
        .long_horizon
        .as_ref()
        .and_then(|lh| lh.completion_gate.clone())
        .unwrap_or_default();

    let completion_gate = CompletionGateConfigToml {
        auto_verify_replay: Some(normalize_gate_mode(&settings.auto_verify_replay)),
        toolchain_gate: Some(normalize_gate_mode(&settings.toolchain_gate)),
        stub_gate: Some(normalize_gate_mode(&settings.stub_gate)),
        max_manifest_rounds: Some(settings.max_manifest_rounds.clamp(1, 32)),
        max_audit_rounds: Some(settings.max_audit_rounds.clamp(1, 32)),
        max_infra_strikes: Some(settings.max_infra_strikes.clamp(1, 16)),
        verify: existing_gate.verify,
        deliverable: existing_gate.deliverable,
        mode: existing_gate.mode,
    };

    store.config.long_horizon = Some(LongHorizonConfigToml {
        enabled: Some(settings.enabled),
        mode: Some(normalize_lht_mode(&settings.mode)),
        progress_via_git: Some(settings.progress_via_git),
        max_nudges_per_item: Some(settings.max_nudges_per_item.clamp(1, 20)),
        blocked_nudges_without_progress: Some(
            settings.blocked_nudges_without_progress.clamp(1, 10),
        ),
        auto_continue: Some(settings.auto_continue),
        max_auto_continue_rounds: Some(settings.max_auto_continue_rounds.clamp(1, 64)),
        reinject_every_steps: store
            .config
            .long_horizon
            .as_ref()
            .and_then(|lh| lh.reinject_every_steps),
        completion_gate: Some(completion_gate),
        macro_loop: Some(MacroLoopConfigToml {
            enabled: Some(settings.macro_loop_enabled),
            max_macro_cycles: Some(settings.macro_loop_max_cycles.clamp(1, 8)),
            max_craft_rounds_per_cycle: Some(settings.macro_loop_max_craft_rounds.clamp(1, 4)),
            auto_enter_craft: Some(normalize_macro_auto_enter(
                &settings.macro_loop_auto_enter_craft,
            )),
            craft_on_small_tasks: Some(settings.macro_loop_craft_on_small_tasks),
            min_checklist_items_for_craft: Some(settings.macro_loop_min_checklist_items.max(1)),
        }),
    });

    tracing::info!("save_lht_settings: writing config");
    store.save().map_err(|e| e.to_string())?;
    ctx.sidecar_restart.notify_one();
    Ok(())
}

// ---------------------------------------------------------------------------
// Hooks settings — desktop panel for `[hooks]` / `[[hooks.hooks]]`
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConditionSettings {
    #[serde(rename = "type")]
    pub condition_type: String,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub conditions: Option<Vec<HookConditionSettings>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEntrySettings {
    pub event: String,
    pub command: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_hook_timeout_ui")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub background: bool,
    #[serde(default = "default_continue_on_error_ui")]
    pub continue_on_error: bool,
    #[serde(default)]
    pub condition: Option<HookConditionSettings>,
}

fn default_hook_timeout_ui() -> u64 {
    30
}

fn default_continue_on_error_ui() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HooksSettings {
    pub enabled: bool,
    pub default_timeout_secs: Option<u64>,
    pub working_dir: Option<String>,
    pub hooks: Vec<HookEntrySettings>,
}

fn hook_event_to_str(event: HookEventToml) -> &'static str {
    match event {
        HookEventToml::SessionStart => "session_start",
        HookEventToml::SessionEnd => "session_end",
        HookEventToml::MessageSubmit => "message_submit",
        HookEventToml::ToolCallBefore => "tool_call_before",
        HookEventToml::ToolCallAfter => "tool_call_after",
        HookEventToml::ModeChange => "mode_change",
        HookEventToml::OnError => "on_error",
        HookEventToml::ShellEnv => "shell_env",
        HookEventToml::PreCompact => "pre_compact",
        HookEventToml::PostCompact => "post_compact",
        HookEventToml::SubagentStart => "subagent_start",
        HookEventToml::SubagentEnd => "subagent_end",
    }
}

/// Returns `(canonical event, optional implicit condition)`.
/// Cursor-style aliases (before_shell, after_shell, before_file_edit, after_file_edit) carry
/// an implicit tool-name filter that must be AND-ed with the user-supplied condition so that
/// the saved config.toml matches the runtime hooks_load.rs alias expansion.
fn hook_event_from_str(raw: &str) -> Result<(HookEventToml, Option<HookConditionToml>), String> {
    let key = raw.trim().to_ascii_lowercase();
    match key.as_str() {
        "session_start" => Ok((HookEventToml::SessionStart, None)),
        "session_end" | "stop" => Ok((HookEventToml::SessionEnd, None)),
        "message_submit" => Ok((HookEventToml::MessageSubmit, None)),
        "tool_call_before" => Ok((HookEventToml::ToolCallBefore, None)),
        "tool_call_after" => Ok((HookEventToml::ToolCallAfter, None)),
        "mode_change" => Ok((HookEventToml::ModeChange, None)),
        "on_error" => Ok((HookEventToml::OnError, None)),
        "shell_env" => Ok((HookEventToml::ShellEnv, None)),
        "pre_compact" => Ok((HookEventToml::PreCompact, None)),
        "post_compact" => Ok((HookEventToml::PostCompact, None)),
        "subagent_start" => Ok((HookEventToml::SubagentStart, None)),
        "subagent_end" => Ok((HookEventToml::SubagentEnd, None)),
        "before_shell" => Ok((
            HookEventToml::ToolCallBefore,
            Some(HookConditionToml::ToolName {
                name: "exec_shell".to_string(),
            }),
        )),
        "after_shell" => Ok((
            HookEventToml::ToolCallAfter,
            Some(HookConditionToml::ToolName {
                name: "exec_shell".to_string(),
            }),
        )),
        "before_file_edit" => Ok((
            HookEventToml::ToolCallBefore,
            Some(HookConditionToml::ToolCategory {
                category: "file_write".to_string(),
            }),
        )),
        "after_file_edit" => Ok((
            HookEventToml::ToolCallAfter,
            Some(HookConditionToml::ToolCategory {
                category: "file_write".to_string(),
            }),
        )),
        other => Err(format!("unknown hook event: {other}")),
    }
}

/// Merge `implicit` condition into `existing` using AND semantics (mirrors hooks_load.rs).
fn merge_implicit_condition_toml(
    existing: &mut Option<HookConditionToml>,
    implicit: Option<HookConditionToml>,
) {
    let Some(implicit) = implicit else { return };
    match existing.take() {
        None => *existing = Some(implicit),
        Some(current) => {
            *existing = Some(HookConditionToml::All {
                conditions: vec![implicit, current],
            });
        }
    }
}

fn hook_condition_to_settings(cond: &HookConditionToml) -> HookConditionSettings {
    match cond {
        HookConditionToml::Always => HookConditionSettings {
            condition_type: "always".to_string(),
            value: None,
            conditions: None,
        },
        HookConditionToml::ToolName { name } => HookConditionSettings {
            condition_type: "tool_name".to_string(),
            value: Some(name.clone()),
            conditions: None,
        },
        HookConditionToml::ToolNameRegex { pattern } => HookConditionSettings {
            condition_type: "tool_name_regex".to_string(),
            value: Some(pattern.clone()),
            conditions: None,
        },
        HookConditionToml::ToolCategory { category } => HookConditionSettings {
            condition_type: "tool_category".to_string(),
            value: Some(category.clone()),
            conditions: None,
        },
        HookConditionToml::Mode { mode } => HookConditionSettings {
            condition_type: "mode".to_string(),
            value: Some(mode.clone()),
            conditions: None,
        },
        HookConditionToml::ExitCode { code } => HookConditionSettings {
            condition_type: "exit_code".to_string(),
            value: Some(code.to_string()),
            conditions: None,
        },
        HookConditionToml::All { conditions } => HookConditionSettings {
            condition_type: "all".to_string(),
            value: None,
            conditions: Some(conditions.iter().map(hook_condition_to_settings).collect()),
        },
        HookConditionToml::Any { conditions } => HookConditionSettings {
            condition_type: "any".to_string(),
            value: None,
            conditions: Some(conditions.iter().map(hook_condition_to_settings).collect()),
        },
    }
}

fn hook_condition_from_settings_inner(
    settings: &HookConditionSettings,
) -> Result<HookConditionToml, String> {
    let value = settings
        .value
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    match settings.condition_type.as_str() {
        "always" => Ok(HookConditionToml::Always),
        "all" => {
            let subs = settings
                .conditions
                .as_ref()
                .filter(|c| !c.is_empty())
                .ok_or_else(|| "all condition requires sub-conditions".to_string())?;
            Ok(HookConditionToml::All {
                conditions: subs
                    .iter()
                    .map(hook_condition_from_settings_inner)
                    .collect::<Result<_, _>>()?,
            })
        }
        "any" => {
            let subs = settings
                .conditions
                .as_ref()
                .filter(|c| !c.is_empty())
                .ok_or_else(|| "any condition requires sub-conditions".to_string())?;
            Ok(HookConditionToml::Any {
                conditions: subs
                    .iter()
                    .map(hook_condition_from_settings_inner)
                    .collect::<Result<_, _>>()?,
            })
        }
        "tool_name" => {
            let name = value.ok_or_else(|| "tool_name condition requires a value".to_string())?;
            Ok(HookConditionToml::ToolName { name })
        }
        "tool_name_regex" => {
            let pattern =
                value.ok_or_else(|| "tool_name_regex condition requires a pattern".to_string())?;
            Ok(HookConditionToml::ToolNameRegex { pattern })
        }
        "tool_category" => {
            let category =
                value.ok_or_else(|| "tool_category condition requires a value".to_string())?;
            Ok(HookConditionToml::ToolCategory { category })
        }
        "mode" => {
            let mode = value.ok_or_else(|| "mode condition requires a value".to_string())?;
            Ok(HookConditionToml::Mode { mode })
        }
        "exit_code" => {
            let raw = value.ok_or_else(|| "exit_code condition requires a value".to_string())?;
            let code: i32 = raw
                .parse()
                .map_err(|_| format!("invalid exit_code: {raw}"))?;
            Ok(HookConditionToml::ExitCode { code })
        }
        other => Err(format!("unknown hook condition type: {other}")),
    }
}

fn hook_condition_from_settings(
    settings: &HookConditionSettings,
) -> Result<Option<HookConditionToml>, String> {
    match settings.condition_type.as_str() {
        "always" => Ok(None),
        "" => Ok(None),
        _ => hook_condition_from_settings_inner(settings).map(Some),
    }
}

fn hooks_settings_from_config(cfg: &ConfigToml) -> HooksSettings {
    let hooks_cfg = cfg.hooks.clone().unwrap_or_default();
    HooksSettings {
        enabled: hooks_cfg.enabled,
        default_timeout_secs: hooks_cfg.default_timeout_secs,
        working_dir: hooks_cfg
            .working_dir
            .as_ref()
            .map(|p| p.display().to_string()),
        hooks: hooks_cfg
            .hooks
            .into_iter()
            .map(|h| HookEntrySettings {
                event: hook_event_to_str(h.event).to_string(),
                command: h.command,
                name: h.name,
                timeout_secs: h.timeout_secs.unwrap_or(30),
                background: h.background,
                continue_on_error: h.continue_on_error,
                condition: h.condition.as_ref().map(hook_condition_to_settings),
            })
            .collect(),
    }
}

#[tauri::command]
pub fn get_hooks_settings() -> Result<HooksSettings, String> {
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    Ok(hooks_settings_from_config(&store.config))
}

#[tauri::command]
pub fn save_hooks_settings(
    settings: HooksSettings,
    ctx: tauri::State<'_, AppContext>,
) -> Result<(), String> {
    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let mut hooks = Vec::with_capacity(settings.hooks.len());
    for entry in settings.hooks {
        let command = entry.command.trim();
        if command.is_empty() {
            continue;
        }
        let (event_toml, implicit_cond) = hook_event_from_str(&entry.event)?;
        let mut condition = entry
            .condition
            .as_ref()
            .map(hook_condition_from_settings)
            .transpose()?
            .flatten();
        merge_implicit_condition_toml(&mut condition, implicit_cond);
        hooks.push(HookToml {
            event: event_toml,
            command: command.to_string(),
            condition,
            // Only persist a non-default timeout so `default_timeout_secs` can still apply.
            timeout_secs: if entry.timeout_secs == 30 {
                None
            } else {
                Some(entry.timeout_secs.max(1))
            },
            background: entry.background,
            continue_on_error: entry.continue_on_error,
            name: entry
                .name
                .as_ref()
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty()),
        });
    }

    let working_dir = settings
        .working_dir
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);

    let audit_jsonl = store
        .config
        .hooks
        .as_ref()
        .and_then(|h| h.audit_jsonl.clone());

    store.config.hooks = Some(HooksConfigToml {
        enabled: settings.enabled,
        default_timeout_secs: settings.default_timeout_secs.filter(|&v| v > 0),
        working_dir,
        audit_jsonl,
        hooks,
    });

    tracing::info!("save_hooks_settings: writing config");
    store.save().map_err(|e| e.to_string())?;
    ctx.sidecar_restart.notify_one();
    Ok(())
}

#[tauri::command]
pub fn restart_sidecar(ctx: tauri::State<'_, AppContext>) -> Result<(), String> {
    ctx.sidecar_restart.notify_one();
    Ok(())
}

// ---------------------------------------------------------------------------
// Composer workspace defaults (`<Documents>/Zagens`, legacy `Zagens`)
// ---------------------------------------------------------------------------

/// Returns the default Composer workspace directory, creating it if needed.
#[tauri::command]
pub fn default_composer_workspace() -> Result<String, String> {
    crate::workspace_defaults::default_composer_workspace()
}

// ---------------------------------------------------------------------------
// pick-rules — `.zagens/pick-rules.md` per workspace (Zagens project rules)
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

/// Restrict IPC workspace roots to paths under the user's home or documents folder.
fn user_scoped_workspace_root(raw: &str) -> Result<PathBuf, String> {
    let base = workspace_root_canonical(raw)?;
    let allowed_roots: Vec<PathBuf> = [dirs::home_dir(), dirs::document_dir()]
        .into_iter()
        .flatten()
        .filter_map(|path| path.canonicalize().ok())
        .collect();
    if allowed_roots.is_empty() {
        return Err("无法解析用户目录".to_string());
    }
    if allowed_roots
        .iter()
        .any(|allowed| base.starts_with(allowed))
    {
        Ok(base)
    } else {
        Err("工作区必须在用户主目录或文档目录下".to_string())
    }
}

fn pick_rules_path_under_workspace(base: &Path) -> PathBuf {
    workspace_meta_file_read(base, "pick-rules.md")
}

/// Read Zagens project rules for a workspace. Returns empty string if the file is missing.
#[tauri::command]
pub fn read_pick_rules(workspace_root: String) -> Result<String, String> {
    let base = user_scoped_workspace_root(&workspace_root)?;
    let path = pick_rules_path_under_workspace(&base);
    if !path.is_file() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).map_err(|e| format!("读取项目规则失败: {e}"))
}

/// Write Zagens project rules. Creates `.zagens/` when needed.
#[tauri::command]
pub fn save_pick_rules(workspace_root: String, content: String) -> Result<(), String> {
    let base = user_scoped_workspace_root(&workspace_root)?;
    let meta = workspace_meta_dir(&base);
    std::fs::create_dir_all(&meta)
        .map_err(|e| format!("创建 {WORKSPACE_META_DIR_NAME} 目录失败: {e}"))?;
    let path = workspace_meta_file_write(&base, "pick-rules.md");

    if content.len() > PICK_RULES_MAX_BYTES {
        return Err(format!(
            "规则内容过长（最大 {} KiB，与 instructions 文件上限一致）",
            PICK_RULES_MAX_BYTES / 1024
        ));
    }

    std::fs::write(&path, content.as_bytes()).map_err(|e| format!("写入项目规则失败: {e}"))?;
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
    let url = format!(
        "http://127.0.0.1:{}/v1/sessions/{}",
        ctx.require_port()?,
        enc
    );
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

    let out_path = crate::export_path::validate_export_json_path(&save_path)?;
    std::fs::write(&out_path, json).map_err(|e| format!("保存失败: {e}"))?;

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
    let runtime_port = ctx.require_port()?;
    let token = &ctx.runtime_token;
    let client = reqwest::Client::new();
    let url = format!(
        "http://127.0.0.1:{runtime_port}/v1/symbol-index/rebuild?workspace={}",
        urlencoding(&workspace)
    );
    eprintln!("[symbol-index] rebuild: POST {url}");
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| {
            let msg = chain_transport_error_cn("请求失败", &e);
            eprintln!("[symbol-index] rebuild failed: {msg}");
            msg
        })?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let msg = format!("索引重建失败 ({status}): {body}");
        eprintln!("[symbol-index] rebuild HTTP error: {msg}");
        return Err(msg);
    }
    eprintln!("[symbol-index] rebuild OK");
    Ok(())
}

// ---------------------------------------------------------------------------
// symbol index management — inspect/manage the workspace symbol index
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SymbolIndexInfo {
    pub status: String,
    pub path: String,
    pub dir: String,
    pub size_bytes: u64,
    pub schema_version: u32,
    pub file_count: usize,
    pub symbol_count: usize,
}

#[tauri::command]
pub fn get_symbol_index_info(workspace: String) -> Result<SymbolIndexInfo, String> {
    let ws = PathBuf::from(workspace.trim());
    if !ws.is_dir() {
        return Err("工作区路径不存在".to_string());
    }
    let meta_dir = workspace_meta_dir_read(&ws);
    let index_path = workspace_meta_file_read(&ws, "symbols.json");

    if !index_path.exists() {
        return Ok(SymbolIndexInfo {
            status: "missing".to_string(),
            path: index_path.to_string_lossy().to_string(),
            dir: meta_dir.to_string_lossy().to_string(),
            size_bytes: 0,
            schema_version: 0,
            file_count: 0,
            symbol_count: 0,
        });
    }

    let meta = std::fs::metadata(&index_path).map_err(|e| format!("无法读取索引文件: {e}"))?;
    let size_bytes = meta.len();
    let raw = std::fs::read_to_string(&index_path).map_err(|e| format!("无法读取索引内容: {e}"))?;

    let index: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("索引 JSON 解析失败: {e}"))?;

    let schema_version = index
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let file_count = index
        .get("files")
        .and_then(|v| v.as_object())
        .map(|o| o.len())
        .unwrap_or(0);
    let symbol_count: usize = index
        .get("files")
        .and_then(|v| v.as_object())
        .map(|files| {
            files
                .values()
                .filter_map(|f| f.get("symbols").and_then(|s| s.as_array()))
                .map(|s| s.len())
                .sum()
        })
        .unwrap_or(0);

    // Check freshness: schema bump or source files newer than the index file.
    const CURRENT_SYMBOL_SCHEMA: u32 = 5;
    let status = if schema_version < CURRENT_SYMBOL_SCHEMA {
        "stale"
    } else {
        let idx_mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut stale = false;
        let walker = WalkBuilder::new(&ws)
            .standard_filters(true)
            .hidden(false)
            .build();
        for entry in walker.filter_map(|e| e.ok()) {
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }
            let ext = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if !matches!(
                ext,
                "rs" | "ts"
                    | "tsx"
                    | "js"
                    | "jsx"
                    | "mjs"
                    | "cjs"
                    | "py"
                    | "pyi"
                    | "go"
                    | "c"
                    | "h"
                    | "cpp"
                    | "cc"
                    | "cxx"
                    | "hpp"
                    | "hxx"
                    | "hh"
                    | "vue"
                    | "svelte"
            ) {
                continue;
            }
            if let Ok(mt) = entry.metadata()
                && let Some(secs) = mt
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                && secs.as_secs() > idx_mtime
            {
                stale = true;
                break;
            }
        }
        if stale { "stale" } else { "fresh" }
    };

    Ok(SymbolIndexInfo {
        status: status.to_string(),
        path: index_path.to_string_lossy().to_string(),
        dir: meta_dir.to_string_lossy().to_string(),
        size_bytes,
        schema_version,
        file_count,
        symbol_count,
    })
}

#[tauri::command]
pub fn delete_symbol_index(workspace: String) -> Result<(), String> {
    let ws = PathBuf::from(workspace.trim());
    if !ws.is_dir() {
        return Err("工作区路径不存在".to_string());
    }
    for rel in [
        "symbols.json",
        ".symbols_fingerprint",
        ".symbols_changes.json",
    ] {
        for root in [workspace_meta_dir(&ws), legacy_workspace_meta_dir(&ws)] {
            let path = root.join(rel);
            if path.exists() {
                std::fs::remove_file(&path).map_err(|e| format!("删除索引文件失败: {e}"))?;
            }
        }
    }
    Ok(())
}

/// Free space on user-data (`~/.zagens`) and optional workspace volumes (for pause-turn UX).
#[tauri::command]
pub fn get_storage_pressure(
    workspace_root: Option<String>,
) -> Result<crate::disk_guard::StoragePressureSnapshot, String> {
    crate::disk_guard::storage_pressure_snapshot(workspace_root.as_deref())
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
