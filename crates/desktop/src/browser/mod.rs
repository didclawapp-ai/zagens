//! Browser pane host (P1 spike): embedded child WebView (B) or windowed WebviewWindow (C).

mod bridge;
mod url_policy;

pub use bridge::{BrowserBridgeUrl, start_browser_bridge};
pub use url_policy::{NavActor, validate_navigation};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::webview::WebviewBuilder;
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use url::Url;

const BLANK: &str = "about:blank";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserMode {
    Embedded,
    Windowed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStateDto {
    pub parent_label: String,
    pub host_label: String,
    pub mode: BrowserMode,
    pub url: String,
    pub title: String,
    pub visible: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSnapshotDto {
    pub url: String,
    pub title: String,
    pub text: String,
    pub nodes: Vec<BrowserA11yNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserA11yNode {
    pub r#ref: String,
    pub role: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl BrowserError {
    pub fn missing() -> Self {
        Self {
            code: "browser_host_missing".into(),
            message: "当前窗口没有 Browser 宿主".into(),
            hint: Some("请打开 Browser 视图后再试".into()),
        }
    }

    pub fn msg(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hint: None,
        }
    }
}

struct HostRecord {
    parent_label: String,
    host_label: String,
    mode: BrowserMode,
    visible: bool,
}

pub struct BrowserHosts {
    inner: Mutex<HashMap<String, HostRecord>>,
}

impl BrowserHosts {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn forget_host_label(&self, host_label: &str) {
        if let Ok(mut g) = self.inner.lock() {
            g.retain(|_, rec| rec.host_label != host_label);
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, HostRecord>>, BrowserError> {
        self.inner
            .lock()
            .map_err(|_| BrowserError::msg("lock_failed", "BrowserHosts 锁失败"))
    }
}

fn profile_dir(parent_label: &str) -> Result<PathBuf, BrowserError> {
    let base =
        dirs::data_dir().ok_or_else(|| BrowserError::msg("no_data_dir", "无法解析用户数据目录"))?;
    Ok(base
        .join("zagens")
        .join("browser-profile")
        .join(sanitize_label(parent_label)))
}

fn sanitize_label(label: &str) -> String {
    label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn host_label_for(parent: &str, mode: BrowserMode) -> String {
    match mode {
        BrowserMode::Embedded => format!("browser-embed-{}", sanitize_label(parent)),
        BrowserMode::Windowed => format!("browser-win-{}", sanitize_label(parent)),
    }
}

fn parse_create_url(raw: Option<&str>) -> Result<Url, BrowserError> {
    let s = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(BLANK);
    validate_navigation(s, NavActor::Human).map_err(|e| BrowserError {
        code: e.code,
        message: e.message,
        hint: None,
    })
}

async fn create_embedded(
    app: &AppHandle,
    parent: &WebviewWindow,
    host_label: &str,
    url: Url,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), BrowserError> {
    let parent_label = parent.label().to_string();
    let win = app.get_window(&parent_label).ok_or_else(|| {
        BrowserError::msg(
            "parent_window_missing",
            "找不到父窗口（unstable get_window）",
        )
    })?;

    if app.get_webview(host_label).is_some() {
        let wv = app.get_webview(host_label).unwrap();
        let _ = wv.set_position(LogicalPosition::new(x, y));
        let _ = wv.set_size(LogicalSize::new(width.max(1.0), height.max(1.0)));
        let _ = wv.show();
        let _ = wv.navigate(url);
        return Ok(());
    }

    let data_dir = profile_dir(&parent_label)?;
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| BrowserError::msg("profile_dir", format!("创建 profile 目录失败: {e}")))?;

    let builder =
        WebviewBuilder::new(host_label, WebviewUrl::External(url)).data_directory(data_dir);

    let w = width.max(1.0);
    let h = height.max(1.0);
    // Must not create webviews on the sync IPC path on Windows — this command is async.
    win.add_child(builder, LogicalPosition::new(x, y), LogicalSize::new(w, h))
        .map_err(|e| BrowserError::msg("embed_failed", format!("嵌入子 WebView 失败: {e}")))?;
    Ok(())
}

async fn create_windowed(
    app: &AppHandle,
    parent: &WebviewWindow,
    host_label: &str,
    url: Url,
) -> Result<(), BrowserError> {
    let parent_label = parent.label().to_string();
    if let Some(existing) = app.get_webview_window(host_label) {
        let _ = existing.navigate(url);
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    let data_dir = profile_dir(&parent_label)?;
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| BrowserError::msg("profile_dir", format!("创建 profile 目录失败: {e}")))?;

    let title = format!("Zagens Browser · {parent_label}");
    WebviewWindowBuilder::new(app, host_label, WebviewUrl::External(url))
        .title(title)
        .inner_size(960.0, 720.0)
        .min_inner_size(480.0, 320.0)
        .center()
        .data_directory(data_dir)
        .build()
        .map_err(|e| BrowserError::msg("window_failed", format!("创建 Browser 窗失败: {e}")))?;
    Ok(())
}

fn read_url_title(app: &AppHandle, rec: &HostRecord) -> (String, String) {
    match rec.mode {
        BrowserMode::Embedded => {
            if let Some(wv) = app.get_webview(&rec.host_label) {
                let url = wv.url().map(|u| u.to_string()).unwrap_or_default();
                return (url, String::new());
            }
        }
        BrowserMode::Windowed => {
            if let Some(w) = app.get_webview_window(&rec.host_label) {
                let url = w.url().map(|u| u.to_string()).unwrap_or_default();
                let title = w.title().unwrap_or_default();
                return (url, title);
            }
        }
    }
    (String::new(), String::new())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCreateArgs {
    /// `embedded` | `windowed` | `auto` (try B, fall back to C). Default `auto`.
    pub mode: Option<String>,
    pub url: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

#[tauri::command]
pub async fn browser_create(
    app: AppHandle,
    window: WebviewWindow,
    hosts: State<'_, BrowserHosts>,
    args: BrowserCreateArgs,
) -> Result<BrowserStateDto, BrowserError> {
    let parent_label = window.label().to_string();
    let url = parse_create_url(args.url.as_deref())?;
    let want = args.mode.as_deref().unwrap_or("auto").to_ascii_lowercase();
    let x = args.x.unwrap_or(0.0);
    let y = args.y.unwrap_or(0.0);
    let width = args.width.unwrap_or(400.0);
    let height = args.height.unwrap_or(600.0);

    // Destroy prior host for this parent (re-create).
    {
        let mut g = hosts.lock()?;
        if let Some(prev) = g.remove(&parent_label) {
            destroy_surface(&app, &prev);
        }
    }

    let (mode, host_label) = match want.as_str() {
        "windowed" | "window" | "c" => {
            let label = host_label_for(&parent_label, BrowserMode::Windowed);
            create_windowed(&app, &window, &label, url.clone()).await?;
            (BrowserMode::Windowed, label)
        }
        "embedded" | "embed" | "b" => {
            let label = host_label_for(&parent_label, BrowserMode::Embedded);
            create_embedded(&app, &window, &label, url.clone(), x, y, width, height).await?;
            (BrowserMode::Embedded, label)
        }
        _ => {
            // auto: prefer B, fall back to C
            let embed_label = host_label_for(&parent_label, BrowserMode::Embedded);
            match create_embedded(
                &app,
                &window,
                &embed_label,
                url.clone(),
                x,
                y,
                width,
                height,
            )
            .await
            {
                Ok(()) => (BrowserMode::Embedded, embed_label),
                Err(embed_err) => {
                    tracing::warn!(
                        target: "zagens_browser",
                        error = %embed_err.message,
                        "embedded BrowserHost failed; falling back to windowed"
                    );
                    let win_label = host_label_for(&parent_label, BrowserMode::Windowed);
                    create_windowed(&app, &window, &win_label, url).await?;
                    (BrowserMode::Windowed, win_label)
                }
            }
        }
    };

    let rec = HostRecord {
        parent_label: parent_label.clone(),
        host_label: host_label.clone(),
        mode,
        visible: true,
    };
    hosts.lock()?.insert(parent_label.clone(), rec);

    Ok(BrowserStateDto {
        parent_label,
        host_label: host_label.clone(),
        mode,
        url: url_string_for(&app, mode, &host_label),
        title: String::new(),
        visible: true,
    })
}

fn url_string_for(app: &AppHandle, mode: BrowserMode, host_label: &str) -> String {
    match mode {
        BrowserMode::Embedded => app
            .get_webview(host_label)
            .and_then(|w| w.url().ok())
            .map(|u| u.to_string())
            .unwrap_or_default(),
        BrowserMode::Windowed => app
            .get_webview_window(host_label)
            .and_then(|w| w.url().ok())
            .map(|u| u.to_string())
            .unwrap_or_default(),
    }
}

fn destroy_surface(app: &AppHandle, rec: &HostRecord) {
    match rec.mode {
        BrowserMode::Embedded => {
            if let Some(wv) = app.get_webview(&rec.host_label) {
                let _ = wv.close();
            }
        }
        BrowserMode::Windowed => {
            if let Some(w) = app.get_webview_window(&rec.host_label) {
                let _ = w.close();
            }
        }
    }
}

#[tauri::command]
pub async fn browser_destroy(
    app: AppHandle,
    window: WebviewWindow,
    hosts: State<'_, BrowserHosts>,
) -> Result<(), BrowserError> {
    let parent = window.label().to_string();
    let mut g = hosts.lock()?;
    if let Some(rec) = g.remove(&parent) {
        destroy_surface(&app, &rec);
    }
    Ok(())
}

/// Called when an agent window closes — must destroy its BrowserHost.
pub fn destroy_for_parent(app: &AppHandle, hosts: &BrowserHosts, parent_label: &str) {
    if let Ok(mut g) = hosts.inner.lock() {
        if let Some(rec) = g.remove(parent_label) {
            destroy_surface(app, &rec);
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserNavigateArgs {
    pub url: String,
    /// `human` (default) | `agent`
    pub actor: Option<String>,
}

#[tauri::command]
pub async fn browser_navigate(
    app: AppHandle,
    window: WebviewWindow,
    hosts: State<'_, BrowserHosts>,
    args: BrowserNavigateArgs,
) -> Result<BrowserStateDto, BrowserError> {
    let parent = window.label().to_string();
    let actor = match args.actor.as_deref().unwrap_or("human") {
        "agent" => NavActor::Agent,
        _ => NavActor::Human,
    };
    let url = validate_navigation(&args.url, actor).map_err(|e| BrowserError {
        code: e.code,
        message: e.message,
        hint: None,
    })?;

    let (mode, host_label) = {
        let g = hosts.lock()?;
        let rec = g.get(&parent).ok_or_else(BrowserError::missing)?;
        (rec.mode, rec.host_label.clone())
    };

    match mode {
        BrowserMode::Embedded => {
            let wv = app
                .get_webview(&host_label)
                .ok_or_else(BrowserError::missing)?;
            wv.navigate(url)
                .map_err(|e| BrowserError::msg("navigate_failed", e.to_string()))?;
        }
        BrowserMode::Windowed => {
            let w = app
                .get_webview_window(&host_label)
                .ok_or_else(BrowserError::missing)?;
            w.navigate(url)
                .map_err(|e| BrowserError::msg("navigate_failed", e.to_string()))?;
        }
    }
    browser_get_state(app, window, hosts).await
}

#[tauri::command]
pub async fn browser_get_state(
    app: AppHandle,
    window: WebviewWindow,
    hosts: State<'_, BrowserHosts>,
) -> Result<BrowserStateDto, BrowserError> {
    let parent = window.label().to_string();
    let g = hosts.lock()?;
    let rec = g.get(&parent).ok_or_else(BrowserError::missing)?;
    let (url, title) = read_url_title(&app, rec);
    Ok(BrowserStateDto {
        parent_label: rec.parent_label.clone(),
        host_label: rec.host_label.clone(),
        mode: rec.mode,
        url,
        title,
        visible: rec.visible,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBoundsArgs {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub visible: Option<bool>,
}

#[tauri::command]
pub async fn browser_set_bounds(
    app: AppHandle,
    window: WebviewWindow,
    hosts: State<'_, BrowserHosts>,
    args: BrowserBoundsArgs,
) -> Result<(), BrowserError> {
    let parent = window.label().to_string();
    let (host_label, mode, visible) = {
        let mut g = hosts.lock()?;
        let rec = g.get_mut(&parent).ok_or_else(BrowserError::missing)?;
        if rec.mode != BrowserMode::Embedded {
            return Ok(());
        }
        let visible = args.visible.unwrap_or(true);
        rec.visible = visible;
        (rec.host_label.clone(), rec.mode, visible)
    };
    let _ = mode;

    let wv = app
        .get_webview(&host_label)
        .ok_or_else(BrowserError::missing)?;
    let _ = wv.set_position(LogicalPosition::new(args.x, args.y));
    let _ = wv.set_size(LogicalSize::new(args.width.max(1.0), args.height.max(1.0)));
    if visible {
        let _ = wv.show();
    } else {
        let _ = wv.hide();
    }
    Ok(())
}

const SNAPSHOT_JS: &str = r#"(function(){
  try {
    var text = document.body ? (document.body.innerText || '').slice(0, 50000) : '';
    var title = document.title || '';
    var url = location.href || '';
    var nodes = [];
    var ref = 0;
    var els = document.querySelectorAll('a,button,input,textarea,select,[role="button"],[role="link"],h1,h2,h3');
    for (var i = 0; i < els.length && nodes.length < 100; i++) {
      var el = els[i];
      var r = 'e' + (++ref);
      try { el.setAttribute('data-zagens-ref', r); } catch (e) {}
      var name = (el.getAttribute('aria-label') || el.innerText || el.value || el.getAttribute('placeholder') || '').trim().slice(0, 120);
      nodes.push({ ref: r, role: el.getAttribute('role') || el.tagName.toLowerCase(), name: name });
    }
    return JSON.stringify({ url: url, title: title, text: text, nodes: nodes });
  } catch (err) {
    return JSON.stringify({ url: location.href || '', title: '', text: String(err), nodes: [] });
  }
})()"#;

#[tauri::command]
pub async fn browser_snapshot(
    app: AppHandle,
    window: WebviewWindow,
    hosts: State<'_, BrowserHosts>,
) -> Result<BrowserSnapshotDto, BrowserError> {
    let parent = window.label().to_string();
    let (host_label, mode) = {
        let g = hosts.lock()?;
        let rec = g.get(&parent).ok_or_else(BrowserError::missing)?;
        (rec.host_label.clone(), rec.mode)
    };

    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let tx = std::sync::Mutex::new(Some(tx));
    let callback = move |result: String| {
        if let Ok(mut guard) = tx.lock() {
            if let Some(sender) = guard.take() {
                let _ = sender.send(result);
            }
        }
    };

    match mode {
        BrowserMode::Embedded => {
            let wv = app
                .get_webview(&host_label)
                .ok_or_else(BrowserError::missing)?;
            wv.eval_with_callback(SNAPSHOT_JS, callback)
                .map_err(|e| BrowserError::msg("eval_failed", e.to_string()))?;
        }
        BrowserMode::Windowed => {
            let w = app
                .get_webview_window(&host_label)
                .ok_or_else(BrowserError::missing)?;
            w.eval_with_callback(SNAPSHOT_JS, callback)
                .map_err(|e| BrowserError::msg("eval_failed", e.to_string()))?;
        }
    }

    let raw = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
        .await
        .map_err(|_| BrowserError::msg("snapshot_timeout", "snapshot 超时"))?
        .map_err(|_| BrowserError::msg("snapshot_canceled", "snapshot 通道关闭"))?;

    #[derive(Deserialize)]
    struct RawSnap {
        url: String,
        title: String,
        text: String,
        nodes: Vec<BrowserA11yNode>,
    }
    let parsed: RawSnap = serde_json::from_str(&raw).unwrap_or(RawSnap {
        url: String::new(),
        title: String::new(),
        text: raw,
        nodes: vec![],
    });
    Ok(BrowserSnapshotDto {
        url: parsed.url,
        title: parsed.title,
        text: parsed.text,
        nodes: parsed.nodes,
    })
}

#[tauri::command]
pub async fn browser_focus_content(
    app: AppHandle,
    window: WebviewWindow,
    hosts: State<'_, BrowserHosts>,
) -> Result<(), BrowserError> {
    let parent = window.label().to_string();
    let g = hosts.lock()?;
    let rec = g.get(&parent).ok_or_else(BrowserError::missing)?;
    match rec.mode {
        BrowserMode::Embedded => {
            if let Some(wv) = app.get_webview(&rec.host_label) {
                let _ = wv.set_focus();
            }
        }
        BrowserMode::Windowed => {
            if let Some(w) = app.get_webview_window(&rec.host_label) {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
    }
    Ok(())
}

fn lookup_host(hosts: &BrowserHosts, parent: &str) -> Result<(BrowserMode, String), BrowserError> {
    let g = hosts.lock()?;
    let rec = g.get(parent).ok_or_else(BrowserError::missing)?;
    Ok((rec.mode, rec.host_label.clone()))
}

async fn eval_snapshot_on_host(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
) -> Result<BrowserSnapshotDto, BrowserError> {
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let tx = std::sync::Mutex::new(Some(tx));
    let callback = move |result: String| {
        if let Ok(mut guard) = tx.lock() {
            if let Some(sender) = guard.take() {
                let _ = sender.send(result);
            }
        }
    };

    match mode {
        BrowserMode::Embedded => {
            let wv = app
                .get_webview(host_label)
                .ok_or_else(BrowserError::missing)?;
            wv.eval_with_callback(SNAPSHOT_JS, callback)
                .map_err(|e| BrowserError::msg("eval_failed", e.to_string()))?;
        }
        BrowserMode::Windowed => {
            let w = app
                .get_webview_window(host_label)
                .ok_or_else(BrowserError::missing)?;
            w.eval_with_callback(SNAPSHOT_JS, callback)
                .map_err(|e| BrowserError::msg("eval_failed", e.to_string()))?;
        }
    }

    let raw = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
        .await
        .map_err(|_| BrowserError::msg("snapshot_timeout", "snapshot 超时"))?
        .map_err(|_| BrowserError::msg("snapshot_canceled", "snapshot 通道关闭"))?;

    #[derive(Deserialize)]
    struct RawSnap {
        url: String,
        title: String,
        text: String,
        nodes: Vec<BrowserA11yNode>,
    }
    let parsed: RawSnap = serde_json::from_str(&raw).unwrap_or(RawSnap {
        url: String::new(),
        title: String::new(),
        text: raw,
        nodes: vec![],
    });
    Ok(BrowserSnapshotDto {
        url: parsed.url,
        title: parsed.title,
        text: parsed.text,
        nodes: parsed.nodes,
    })
}

/// Agent/bridge navigate (NavActor::Agent). Does not create a host.
pub async fn agent_navigate(
    app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
    url_raw: &str,
) -> Result<BrowserStateDto, BrowserError> {
    let url = validate_navigation(url_raw, NavActor::Agent).map_err(|e| BrowserError {
        code: e.code,
        message: e.message,
        hint: None,
    })?;
    let (mode, host_label) = lookup_host(hosts, parent_label)?;
    match mode {
        BrowserMode::Embedded => {
            let wv = app
                .get_webview(&host_label)
                .ok_or_else(BrowserError::missing)?;
            wv.navigate(url)
                .map_err(|e| BrowserError::msg("navigate_failed", e.to_string()))?;
        }
        BrowserMode::Windowed => {
            let w = app
                .get_webview_window(&host_label)
                .ok_or_else(BrowserError::missing)?;
            w.navigate(url)
                .map_err(|e| BrowserError::msg("navigate_failed", e.to_string()))?;
        }
    }
    let (url_s, title) = {
        let g = hosts.lock()?;
        let rec = g.get(parent_label).ok_or_else(BrowserError::missing)?;
        read_url_title(app, rec)
    };
    Ok(BrowserStateDto {
        parent_label: parent_label.to_string(),
        host_label,
        mode,
        url: url_s,
        title,
        visible: true,
    })
}

pub async fn agent_snapshot(
    app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
) -> Result<BrowserSnapshotDto, BrowserError> {
    let (mode, host_label) = lookup_host(hosts, parent_label)?;
    eval_snapshot_on_host(app, mode, &host_label).await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTextDto {
    pub url: String,
    pub title: String,
    pub text: String,
}

pub async fn agent_get_text(
    app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
) -> Result<BrowserTextDto, BrowserError> {
    let snap = agent_snapshot(app, hosts, parent_label).await?;
    Ok(BrowserTextDto {
        url: snap.url,
        title: snap.title,
        text: snap.text,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserConsoleDto {
    pub lines: Vec<String>,
    pub note: String,
}

pub async fn agent_console_tail(
    _app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
    _limit: usize,
) -> Result<BrowserConsoleDto, BrowserError> {
    let _ = lookup_host(hosts, parent_label)?;
    Ok(BrowserConsoleDto {
        lines: vec![],
        note: "console capture not wired in P1 spike; host exists".into(),
    })
}
