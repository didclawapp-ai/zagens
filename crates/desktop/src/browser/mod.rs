//! Browser pane host (P1/P2): embedded child WebView (B) or windowed WebviewWindow (C).
//!
//! **IPC note:** Commands take `tauri::Webview`, not `WebviewWindow`. Once an embedded
//! child is attached, Tauri's `Window::is_webview_window()` becomes false (any webview
//! whose label ≠ window label), and every `WebviewWindow` CommandArg fails with
//! `current webview is not a WebviewWindow` — breaking the whole app, not just Browser.

mod bridge;
mod cdp;
mod cdp_history;
mod cdp_interact;
mod cdp_snapshot;
pub mod interact;
mod nav_actor;
mod new_window;
mod prefs_store;
pub mod preview;
mod screenshot;
mod scripts;
mod url_policy;

pub use bridge::{BrowserBridgeUrl, start_browser_bridge};
pub use preview::PreviewProcess;
pub use url_policy::{
    NavActor, agent_external_https_host, is_loopback_host, security_kind, validate_human_url,
    validate_navigation_with,
};

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tauri::webview::{PageLoadEvent, WebviewBuilder};
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, State, WebviewUrl, WebviewWindowBuilder,
    Window,
};
use url::Url;

use nav_actor::NavPolicyChain;

use crate::window_registry::WindowRegistry;
use scripts::{
    CONSOLE_CLEAR_JS, CONSOLE_HOOK_INIT, CONSOLE_HOOK_JS, CONSOLE_TAIL_JS, HISTORY_BACK_JS,
    HISTORY_FORWARD_JS, normalize_eval_json, parse_snapshot_json, snapshot_js, wait_check_js,
};
use url_policy::{NavOpts, normalize_host};

const BLANK: &str = "about:blank";
pub(crate) const BLANK_URL: &str = BLANK;
const STATE_EVENT: &str = "browser://state";
const NAV_BLOCKED_EVENT: &str = "browser://nav_blocked";

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
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    /// `blank` | `loopback` | `external` | `file` | `unknown`
    pub security: String,
    pub persist_profile: bool,
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
pub struct BrowserSnapshotDto {
    pub url: String,
    pub title: String,
    pub text: String,
    pub nodes: Vec<BrowserA11yNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iframe_note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Optional structured detail (e.g. window candidates for ambiguous routing).
    /// Boxed so `Result<_, BrowserError>` stays under clippy's `result_large_err` limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Box<serde_json::Value>>,
}

impl BrowserError {
    pub fn missing() -> Self {
        Self {
            code: "browser_host_missing".into(),
            message: "当前窗口没有 Browser 宿主".into(),
            hint: Some("请打开 Browser 视图后再试".into()),
            detail: None,
        }
    }

    pub fn msg(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hint: None,
            detail: None,
        }
    }

    /// Returned when a HostRecord::creating placeholder is hit by a
    /// concurrent browser operation — caller should retry shortly.
    pub fn busy(parent_label: &str) -> Self {
        Self {
            code: "browser_creating".into(),
            message: format!("Browser 宿主正在创建中（{parent_label}），请稍后重试"),
            hint: None,
            detail: None,
        }
    }

    pub(crate) fn from_policy(e: url_policy::UrlPolicyError) -> Self {
        let hint = match e.code.as_str() {
            "agent_external_needs_ask" => {
                Some("审批外站导航，或在 Browser 面板点「允许当前域名（Agent）」后再试".into())
            }
            "file_needs_workspace" => Some("先在 Composer 选择 workspace，再打开 file://".into()),
            "file_escape" => Some("仅允许 workspace 内文件（canonicalize 后须在根目录下）".into()),
            _ => None,
        };
        Self {
            code: e.code,
            message: e.message,
            hint,
            detail: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserPrefsDto {
    pub persist_profile: bool,
    pub allow_private_lan: bool,
    pub allowlist: Vec<String>,
    pub yolo: bool,
}

struct HostRecord {
    parent_label: String,
    host_label: String,
    mode: BrowserMode,
    visible: bool,
    loading: bool,
    persist_profile: bool,
    /// Chrome-driven history for can_go_back / can_go_forward.
    history: Vec<String>,
    history_idx: usize,
    /// When set (Windows CDP), drives back/forward + chrome flags.
    cdp_nav: Option<cdp_history::CdpNavHistory>,
    /// True while browser_create is awaiting the async WebView construction
    /// (between the first lock release and the final lock acquire).
    creating: bool,
    /// Agent vs human URL policy for in-page / programmatic navigations.
    nav_policy: NavPolicyChain,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NavBlockedDto {
    parent_label: String,
    url: String,
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
    /// `human` | `agent`
    actor: String,
}

pub struct BrowserHosts {
    inner: Mutex<HashMap<String, HostRecord>>,
    /// Loaded from `~/.zagens/browser/prefs.json` — survives app restarts.
    pub(crate) persistent_allowlist: Mutex<HashSet<String>>,
    /// In-memory only — cleared when the desktop app exits.
    pub(crate) session_allowlist: Mutex<HashSet<String>>,
    allow_private_lan: Mutex<bool>,
    default_persist: Mutex<bool>,
    /// Independent of global YOLO — mirrored to sidecar env at spawn.
    browser_yolo: Mutex<bool>,
}

impl BrowserHosts {
    pub fn new() -> Self {
        let persisted = prefs_store::load();
        let persistent_allowlist = persisted
            .allowlist
            .into_iter()
            .map(|h| normalize_host(&h))
            .filter(|h| !h.is_empty())
            .collect::<HashSet<_>>();
        Self {
            inner: Mutex::new(HashMap::new()),
            persistent_allowlist: Mutex::new(persistent_allowlist),
            session_allowlist: Mutex::new(HashSet::new()),
            allow_private_lan: Mutex::new(persisted.allow_private_lan),
            default_persist: Mutex::new(true),
            browser_yolo: Mutex::new(persisted.yolo),
        }
    }

    pub(crate) fn persist_prefs_disk(&self) {
        let allowlist = self
            .persistent_allowlist
            .lock()
            .map(|g| {
                let mut v: Vec<String> = g.iter().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default();
        let allow_private_lan = self.allow_private_lan.lock().map(|g| *g).unwrap_or(false);
        let yolo = self.browser_yolo();
        prefs_store::save(&prefs_store::PersistedBrowserPrefs {
            allowlist,
            allow_private_lan,
            yolo,
        });
    }

    /// Parent window labels that currently have a ready BrowserHost.
    pub(crate) fn ready_host_parents(&self) -> Vec<String> {
        self.inner
            .lock()
            .ok()
            .map(|g| {
                g.iter()
                    .filter(|(_, rec)| !rec.creating && !rec.host_label.is_empty())
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn effective_allowlist(&self) -> Result<Vec<String>, BrowserError> {
        let mut merged = self
            .persistent_allowlist
            .lock()
            .map_err(|_| BrowserError::msg("lock_failed", "persistent allowlist 锁失败"))?
            .clone();
        let session = self
            .session_allowlist
            .lock()
            .map_err(|_| BrowserError::msg("lock_failed", "session allowlist 锁失败"))?;
        merged.extend(session.iter().cloned());
        let mut v: Vec<String> = merged.into_iter().collect();
        v.sort();
        v.dedup();
        Ok(v)
    }

    /// Add host to in-memory session allowlist (normalized). Not written to disk.
    pub(crate) fn allow_host_session(&self, host: &str) -> Result<String, BrowserError> {
        let host = normalize_host(host);
        if host.is_empty() {
            return Err(BrowserError::msg("no_host", "主机名为空"));
        }
        if is_loopback_host(&host) {
            return Ok(host);
        }
        self.session_allowlist
            .lock()
            .map_err(|_| BrowserError::msg("lock_failed", "session allowlist 锁失败"))?
            .insert(host.clone());
        Ok(host)
    }

    /// Add host to persistent allowlist and save prefs.json.
    #[allow(dead_code)] // reserved for explicit UI "always allow this domain"
    pub(crate) fn allow_host_persistent(&self, host: &str) -> Result<String, BrowserError> {
        let host = normalize_host(host);
        if host.is_empty() {
            return Err(BrowserError::msg("no_host", "主机名为空"));
        }
        if is_loopback_host(&host) {
            return Ok(host);
        }
        self.persistent_allowlist
            .lock()
            .map_err(|_| BrowserError::msg("lock_failed", "persistent allowlist 锁失败"))?
            .insert(host.clone());
        self.persist_prefs_disk();
        Ok(host)
    }

    /// Back-compat: session-only allow (UI / bridge manual allow).
    pub(crate) fn allow_host_name(&self, host: &str) -> Result<String, BrowserError> {
        self.allow_host_session(host)
    }

    pub fn browser_yolo(&self) -> bool {
        self.browser_yolo.lock().map(|g| *g).unwrap_or(false)
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

    pub(crate) fn nav_opts(&self) -> Result<(Vec<String>, bool), BrowserError> {
        let allow = self.effective_allowlist()?;
        let lan = *self
            .allow_private_lan
            .lock()
            .map_err(|_| BrowserError::msg("lock_failed", "lan 锁失败"))?;
        Ok((allow, lan))
    }

    fn default_persist(&self) -> bool {
        self.default_persist.lock().map(|g| *g).unwrap_or(true)
    }

    /// Begin an agent navigation chain (navigate / click / type).
    pub(crate) fn begin_agent_nav_chain(&self, parent: &str) {
        if let Ok(mut g) = self.inner.lock()
            && let Some(rec) = g.get_mut(parent)
        {
            rec.nav_policy.begin_agent();
        }
    }
}

fn profile_dir(parent_label: &str, persist: bool) -> Result<PathBuf, BrowserError> {
    let base =
        dirs::data_dir().ok_or_else(|| BrowserError::msg("no_data_dir", "无法解析用户数据目录"))?;
    let root = base.join("zagens").join("browser-profile");
    if persist {
        Ok(root.join(sanitize_label(parent_label)))
    } else {
        Ok(root.join("session").join(sanitize_label(parent_label)))
    }
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

fn workspace_path_for(app: &AppHandle, parent: &str) -> Option<PathBuf> {
    let registry = app.state::<WindowRegistry>();
    registry
        .primary_workspace(parent)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

fn parse_create_url(
    app: &AppHandle,
    parent: &str,
    raw: Option<&str>,
    hosts: &BrowserHosts,
) -> Result<Url, BrowserError> {
    let s = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(BLANK);
    let (allow, lan) = hosts.nav_opts()?;
    let ws = workspace_path_for(app, parent);
    let opts = NavOpts {
        allowlist: &allow,
        allow_private_lan: lan,
        workspace_root: ws.as_deref(),
    };
    validate_human_url(s, &opts).map_err(BrowserError::from_policy)
}

fn push_history(rec: &mut HostRecord, url: &str) {
    let url = url.trim();
    if url.is_empty() {
        return;
    }
    if rec.history.get(rec.history_idx).map(String::as_str) == Some(url) {
        return;
    }
    if !rec.history.is_empty() && rec.history_idx + 1 < rec.history.len() {
        rec.history.truncate(rec.history_idx + 1);
    }
    rec.history.push(url.to_string());
    rec.history_idx = rec.history.len().saturating_sub(1);
}

fn state_from_rec(app: &AppHandle, rec: &HostRecord) -> BrowserStateDto {
    let (url, title) = read_url_title(app, rec);
    let url = if url.is_empty() {
        rec.history
            .get(rec.history_idx)
            .cloned()
            .unwrap_or_default()
    } else {
        url
    };
    let (can_go_back, can_go_forward) = if let Some(ref nav) = rec.cdp_nav {
        (nav.can_go_back(), nav.can_go_forward())
    } else {
        (rec.history_idx > 0, rec.history_idx + 1 < rec.history.len())
    };
    BrowserStateDto {
        parent_label: rec.parent_label.clone(),
        host_label: rec.host_label.clone(),
        mode: rec.mode,
        security: security_kind(&url).to_string(),
        url,
        title,
        visible: rec.visible,
        loading: rec.loading,
        can_go_back,
        can_go_forward,
        persist_profile: rec.persist_profile,
    }
}

fn emit_state(app: &AppHandle, parent: &str, state: &BrowserStateDto) {
    let _ = app.emit_to(parent, STATE_EVENT, state);
}

fn apply_page_load(rec: &mut HostRecord, started: bool, finished: bool, url: &str) {
    rec.loading = !finished;
    if started {
        rec.nav_policy.on_page_started();
    }
    if finished {
        rec.nav_policy.on_page_finished();
        if !url.is_empty() {
            push_history(rec, url);
        }
    }
}

/// Shared url_policy gate for `on_navigation` and `window.open`.
pub(crate) fn gate_navigation(app: &AppHandle, parent_label: &str, url_str: &str) -> bool {
    if url_str.eq_ignore_ascii_case(BLANK) {
        return true;
    }
    let hosts = app.state::<BrowserHosts>();
    let actor = match hosts.inner.lock() {
        Ok(mut g) => g
            .get_mut(parent_label)
            .map(|rec| rec.nav_policy.expire_if_needed())
            .unwrap_or(NavActor::Human),
        Err(_) => return false,
    };
    let (allow, lan) = match hosts.nav_opts() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let ws = workspace_path_for(app, parent_label);
    let opts = NavOpts {
        allowlist: &allow,
        allow_private_lan: lan,
        workspace_root: ws.as_deref(),
    };
    match validate_navigation_with(url_str, actor, &opts) {
        Ok(_) => true,
        Err(e) => {
            let err = BrowserError::from_policy(e);
            tracing::warn!(
                target: "zagens_browser",
                parent = %parent_label,
                url = %url_str,
                code = %err.code,
                "browser navigation blocked by url_policy"
            );
            let actor_s = match actor {
                NavActor::Human => "human",
                NavActor::Agent => "agent",
            };
            let dto = NavBlockedDto {
                parent_label: parent_label.to_string(),
                url: url_str.to_string(),
                code: err.code.clone(),
                message: err.message.clone(),
                hint: err.hint.clone(),
                actor: actor_s.into(),
            };
            let _ = app.emit_to(parent_label, NAV_BLOCKED_EVENT, &dto);
            false
        }
    }
}

/// Navigate the Browser host in-place (embedded or windowed).
pub(crate) fn navigate_host_url(
    app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
    url: Url,
) -> Result<(), BrowserError> {
    let url_str = url.to_string();
    let (mode, host_label) = {
        let mut g = hosts.lock()?;
        let rec = g.get_mut(parent_label).ok_or_else(BrowserError::missing)?;
        if rec.creating {
            return Err(BrowserError::busy(parent_label));
        }
        push_history(rec, &url_str);
        rec.loading = true;
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
    Ok(())
}

fn attach_page_load(
    app: &AppHandle,
    parent_label: String,
) -> impl Fn(tauri::Webview, tauri::webview::PageLoadPayload<'_>) + Send + Sync + 'static {
    let app = app.clone();
    move |webview, payload| {
        let hosts = app.state::<BrowserHosts>();
        let url = payload.url().to_string();
        let finished = matches!(payload.event(), PageLoadEvent::Finished);
        let started = matches!(payload.event(), PageLoadEvent::Started);
        if started {
            let _ = webview.eval(CONSOLE_CLEAR_JS);
            let _ = webview.eval(CONSOLE_HOOK_JS);
        }
        if finished {
            let _ = webview.eval(CONSOLE_HOOK_JS);
            if cdp_history::is_available() {
                let app_hist = app.clone();
                let parent_hist = parent_label.clone();
                let (mode, host_label) = {
                    let Ok(g) = hosts.inner.lock() else {
                        return;
                    };
                    let Some(rec) = g.get(&parent_label) else {
                        return;
                    };
                    if rec.creating || rec.host_label != webview.label() {
                        return;
                    }
                    (rec.mode, rec.host_label.clone())
                };
                tauri::async_runtime::spawn(async move {
                    if let Ok(nav) =
                        cdp_history::fetch_navigation_history(&app_hist, mode, &host_label).await
                    {
                        let hosts = app_hist.state::<BrowserHosts>();
                        if let Ok(mut g) = hosts.inner.lock()
                            && let Some(rec) = g.get_mut(&parent_hist)
                        {
                            let (urls, idx) = nav.to_chrome_history();
                            rec.cdp_nav = Some(nav);
                            rec.history = urls;
                            rec.history_idx = idx;
                        }
                        if let Ok(g) = hosts.inner.lock()
                            && let Some(rec) = g.get(&parent_hist)
                        {
                            let dto = state_from_rec(&app_hist, rec);
                            emit_state(&app_hist, &parent_hist, &dto);
                        }
                    }
                });
            }
        }
        if let Ok(mut g) = hosts.inner.lock()
            && let Some(rec) = g.get_mut(&parent_label)
        {
            if rec.creating || rec.host_label != webview.label() {
                return;
            }
            apply_page_load(rec, started, finished, &url);
        }
        if let Ok(g) = hosts.inner.lock()
            && let Some(rec) = g.get(&parent_label)
        {
            let dto = state_from_rec(&app, rec);
            emit_state(&app, &parent_label, &dto);
        }
    }
}

/// Gate every navigation (address bar, agent tools, in-page link / JS redirects)
/// through `url_policy` (§6.1 / §11 A1–A2). Returning `false` cancels the navigation.
fn attach_navigation(
    app: &AppHandle,
    parent_label: String,
) -> impl Fn(&Url) -> bool + Send + 'static {
    let app = app.clone();
    move |url: &Url| gate_navigation(&app, &parent_label, url.as_str())
}

#[allow(clippy::too_many_arguments)]
async fn create_embedded(
    app: &AppHandle,
    parent: &Window,
    host_label: &str,
    url: Url,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    persist: bool,
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

    let data_dir = profile_dir(&parent_label, persist)?;
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| BrowserError::msg("profile_dir", format!("创建 profile 目录失败: {e}")))?;

    let builder = WebviewBuilder::new(host_label, WebviewUrl::External(url))
        .data_directory(data_dir)
        .initialization_script(CONSOLE_HOOK_INIT)
        .on_navigation(attach_navigation(app, parent_label.clone()))
        .on_new_window(new_window::attach_new_window(
            app.clone(),
            parent_label.clone(),
        ))
        .on_page_load(attach_page_load(app, parent_label));

    let w = width.max(1.0);
    let h = height.max(1.0);
    win.add_child(builder, LogicalPosition::new(x, y), LogicalSize::new(w, h))
        .map_err(|e| BrowserError::msg("embed_failed", format!("嵌入子 WebView 失败: {e}")))?;
    Ok(())
}

async fn create_windowed(
    app: &AppHandle,
    parent: &Window,
    host_label: &str,
    url: Url,
    persist: bool,
) -> Result<(), BrowserError> {
    let parent_label = parent.label().to_string();
    if let Some(existing) = app.get_webview_window(host_label) {
        let _ = existing.navigate(url);
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    let data_dir = profile_dir(&parent_label, persist)?;
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| BrowserError::msg("profile_dir", format!("创建 profile 目录失败: {e}")))?;

    let title = format!("Zagens Browser · {parent_label}");
    WebviewWindowBuilder::new(app, host_label, WebviewUrl::External(url))
        .title(title)
        .inner_size(960.0, 720.0)
        .min_inner_size(480.0, 320.0)
        .center()
        .data_directory(data_dir)
        .initialization_script(CONSOLE_HOOK_INIT)
        .on_navigation(attach_navigation(app, parent_label.clone()))
        .on_new_window(new_window::attach_new_window(
            app.clone(),
            parent_label.clone(),
        ))
        .on_page_load({
            let app = app.clone();
            let parent_label = parent_label.clone();
            move |window, payload| {
                let hosts = app.state::<BrowserHosts>();
                let url = payload.url().to_string();
                let finished = matches!(payload.event(), PageLoadEvent::Finished);
                let started = matches!(payload.event(), PageLoadEvent::Started);
                if started {
                    let _ = window.eval(CONSOLE_CLEAR_JS);
                    let _ = window.eval(CONSOLE_HOOK_JS);
                }
                if finished {
                    let _ = window.eval(CONSOLE_HOOK_JS);
                }
                if let Ok(mut g) = hosts.inner.lock()
                    && let Some(rec) = g.get_mut(&parent_label)
                {
                    if rec.host_label != window.label() {
                        return;
                    }
                    apply_page_load(rec, started, finished, &url);
                }
                if let Ok(g) = hosts.inner.lock()
                    && let Some(rec) = g.get(&parent_label)
                {
                    let dto = state_from_rec(&app, rec);
                    emit_state(&app, &parent_label, &dto);
                }
            }
        })
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
    pub persist_profile: Option<bool>,
}

#[tauri::command]
pub async fn browser_create(
    app: AppHandle,
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
    args: BrowserCreateArgs,
) -> Result<BrowserStateDto, BrowserError> {
    let window = webview.window();
    let parent_label = window.label().to_string();
    let url = parse_create_url(&app, &parent_label, args.url.as_deref(), &hosts)?;
    let want = args.mode.as_deref().unwrap_or("auto").to_ascii_lowercase();
    let x = args.x.unwrap_or(0.0);
    let y = args.y.unwrap_or(0.0);
    let width = args.width.unwrap_or(400.0);
    let height = args.height.unwrap_or(600.0);
    let persist = args
        .persist_profile
        .unwrap_or_else(|| hosts.default_persist());

    {
        let mut g = hosts.lock()?;
        if let Some(prev) = g.remove(&parent_label) {
            destroy_surface(&app, &prev);
        }
        // Insert a placeholder so concurrent operations see "busy" instead of
        // "missing" while we await the async WebView construction below.
        g.insert(
            parent_label.clone(),
            HostRecord {
                parent_label: parent_label.clone(),
                host_label: String::new(),
                mode: BrowserMode::Embedded,
                visible: false,
                loading: true,
                persist_profile: persist,
                history: Vec::new(),
                history_idx: 0,
                cdp_nav: None,
                creating: true,
                nav_policy: NavPolicyChain::default(),
            },
        );
    }

    let create_result: Result<(BrowserMode, String), BrowserError> = async {
        match want.as_str() {
            "windowed" | "window" | "c" => {
                let label = host_label_for(&parent_label, BrowserMode::Windowed);
                create_windowed(&app, &window, &label, url.clone(), persist).await?;
                Ok((BrowserMode::Windowed, label))
            }
            "embedded" | "embed" | "b" => {
                let label = host_label_for(&parent_label, BrowserMode::Embedded);
                create_embedded(
                    &app,
                    &window,
                    &label,
                    url.clone(),
                    x,
                    y,
                    width,
                    height,
                    persist,
                )
                .await?;
                Ok((BrowserMode::Embedded, label))
            }
            _ => {
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
                    persist,
                )
                .await
                {
                    Ok(()) => Ok((BrowserMode::Embedded, embed_label)),
                    Err(embed_err) => {
                        tracing::warn!(
                            target: "zagens_browser",
                            error = %embed_err.message,
                            "embedded BrowserHost failed; falling back to windowed"
                        );
                        let win_label = host_label_for(&parent_label, BrowserMode::Windowed);
                        create_windowed(&app, &window, &win_label, url, persist).await?;
                        Ok((BrowserMode::Windowed, win_label))
                    }
                }
            }
        }
    }
    .await;

    let (mode, host_label) = match create_result {
        Ok(v) => v,
        Err(e) => {
            // A3: drop creating placeholder so callers do not see permanent browser_creating.
            if let Ok(mut g) = hosts.lock() {
                g.remove(&parent_label);
            }
            return Err(e);
        }
    };

    let initial_url = url_string_for(&app, mode, &host_label);
    let mut rec = HostRecord {
        parent_label: parent_label.clone(),
        host_label: host_label.clone(),
        mode,
        visible: true,
        loading: true,
        persist_profile: persist,
        history: Vec::new(),
        history_idx: 0,
        cdp_nav: None,
        creating: false,
        nav_policy: NavPolicyChain::default(),
    };
    if !initial_url.is_empty() {
        push_history(&mut rec, &initial_url);
    } else {
        push_history(&mut rec, BLANK);
    }
    let dto = state_from_rec(&app, &rec);
    hosts.lock()?.insert(parent_label.clone(), rec);
    emit_state(&app, &parent_label, &dto);
    Ok(dto)
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
    if !rec.persist_profile
        && let Ok(dir) = profile_dir(&rec.parent_label, false)
    {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tauri::command]
pub async fn browser_destroy(
    app: AppHandle,
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
) -> Result<(), BrowserError> {
    let window = webview.window();
    let parent = window.label().to_string();
    let mut g = hosts.lock()?;
    if let Some(rec) = g.remove(&parent) {
        destroy_surface(&app, &rec);
    }
    Ok(())
}

/// Called when an agent window closes — must destroy its BrowserHost.
pub fn destroy_for_parent(app: &AppHandle, hosts: &BrowserHosts, parent_label: &str) {
    if let Ok(mut g) = hosts.inner.lock()
        && let Some(rec) = g.remove(parent_label)
    {
        destroy_surface(app, &rec);
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
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
    args: BrowserNavigateArgs,
) -> Result<BrowserStateDto, BrowserError> {
    let window = webview.window();
    let parent = window.label().to_string();
    let actor = match args.actor.as_deref().unwrap_or("human") {
        "agent" => NavActor::Agent,
        _ => NavActor::Human,
    };
    let (allow, lan) = hosts.nav_opts()?;
    let ws = workspace_path_for(&app, &parent);
    let opts = NavOpts {
        allowlist: &allow,
        allow_private_lan: lan,
        workspace_root: ws.as_deref(),
    };
    let url =
        validate_navigation_with(&args.url, actor, &opts).map_err(BrowserError::from_policy)?;
    let url_str = url.to_string();

    let (mode, host_label) = {
        let mut g = hosts.lock()?;
        let rec = g.get_mut(&parent).ok_or_else(BrowserError::missing)?;
        if rec.creating {
            return Err(BrowserError::busy(&parent));
        }
        match actor {
            NavActor::Agent => rec.nav_policy.begin_agent(),
            NavActor::Human => rec.nav_policy.begin_human(),
        }
        push_history(rec, &url_str);
        rec.loading = true;
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
    browser_get_state(app, webview, hosts).await
}

#[tauri::command]
pub async fn browser_get_state(
    app: AppHandle,
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
) -> Result<BrowserStateDto, BrowserError> {
    let window = webview.window();
    let parent = window.label().to_string();
    let g = hosts.lock()?;
    let rec = g.get(&parent).ok_or_else(BrowserError::missing)?;
    Ok(state_from_rec(&app, rec))
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
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
    args: BrowserBoundsArgs,
) -> Result<(), BrowserError> {
    let window = webview.window();
    let parent = window.label().to_string();
    let (host_label, visible) = {
        let mut g = hosts.lock()?;
        let rec = g.get_mut(&parent).ok_or_else(BrowserError::missing)?;
        if rec.mode != BrowserMode::Embedded {
            return Ok(());
        }
        let visible = args.visible.unwrap_or(true);
        rec.visible = visible;
        (rec.host_label.clone(), visible)
    };

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

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSnapshotArgs {
    pub include_screenshot: Option<bool>,
}

#[tauri::command]
pub async fn browser_snapshot(
    app: AppHandle,
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
    args: Option<BrowserSnapshotArgs>,
) -> Result<BrowserSnapshotDto, BrowserError> {
    let window = webview.window();
    let parent = window.label().to_string();
    let (host_label, mode) = {
        let g = hosts.lock()?;
        let rec = g.get(&parent).ok_or_else(BrowserError::missing)?;
        (rec.host_label.clone(), rec.mode)
    };

    let mut snap = eval_snapshot_on_host(&app, mode, &host_label).await?;
    let want_shot = args.unwrap_or_default().include_screenshot.unwrap_or(false);
    if want_shot {
        match screenshot::capture_screenshot_data_url(&app, mode, &host_label).await {
            Ok(data_url) => {
                snap.screenshot = Some(data_url);
                snap.screenshot_note = None;
            }
            Err(e) => {
                snap.screenshot = None;
                snap.screenshot_note = Some(format!("{} ({})", e.message, e.code));
            }
        }
    }
    Ok(snap)
}

#[tauri::command]
pub async fn browser_focus_content(
    app: AppHandle,
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
) -> Result<(), BrowserError> {
    let window = webview.window();
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

#[tauri::command]
pub async fn browser_reload(
    app: AppHandle,
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
) -> Result<BrowserStateDto, BrowserError> {
    let window = webview.window();
    let parent = window.label().to_string();
    let (mode, host_label) = {
        let mut g = hosts.lock()?;
        let rec = g.get_mut(&parent).ok_or_else(BrowserError::missing)?;
        rec.loading = true;
        (rec.mode, rec.host_label.clone())
    };
    match mode {
        BrowserMode::Embedded => {
            let wv = app
                .get_webview(&host_label)
                .ok_or_else(BrowserError::missing)?;
            wv.reload()
                .map_err(|e| BrowserError::msg("reload_failed", e.to_string()))?;
        }
        BrowserMode::Windowed => {
            let w = app
                .get_webview_window(&host_label)
                .ok_or_else(BrowserError::missing)?;
            w.reload()
                .map_err(|e| BrowserError::msg("reload_failed", e.to_string()))?;
        }
    }
    browser_get_state(app, webview, hosts).await
}

#[tauri::command]
pub async fn browser_back(
    app: AppHandle,
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
) -> Result<BrowserStateDto, BrowserError> {
    let window = webview.window();
    history_step(&app, &window, &hosts, -1).await
}

#[tauri::command]
pub async fn browser_forward(
    app: AppHandle,
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
) -> Result<BrowserStateDto, BrowserError> {
    let window = webview.window();
    history_step(&app, &window, &hosts, 1).await
}

async fn history_step(
    app: &AppHandle,
    window: &Window,
    hosts: &BrowserHosts,
    delta: i32,
) -> Result<BrowserStateDto, BrowserError> {
    let parent = window.label().to_string();

    if cdp_history::is_available() {
        let (mode, host_label) = lookup_host(hosts, &parent)?;
        let cdp_result = if delta < 0 {
            cdp_history::history_back(app, mode, &host_label).await
        } else {
            cdp_history::history_forward(app, mode, &host_label).await
        };
        match cdp_result {
            Ok(Some(_)) => {
                {
                    let mut g = hosts.lock()?;
                    if let Some(rec) = g.get_mut(&parent) {
                        rec.loading = true;
                    }
                }
                if let Ok(nav) = cdp_history::fetch_navigation_history(app, mode, &host_label).await
                {
                    let mut g = hosts.lock()?;
                    if let Some(rec) = g.get_mut(&parent) {
                        let (urls, idx) = nav.to_chrome_history();
                        rec.cdp_nav = Some(nav);
                        rec.history = urls;
                        rec.history_idx = idx;
                    }
                }
                let g = hosts.lock()?;
                let rec = g.get(&parent).ok_or_else(BrowserError::missing)?;
                let dto = state_from_rec(app, rec);
                emit_state(app, &parent, &dto);
                return Ok(dto);
            }
            Ok(None) => {
                let g = hosts.lock()?;
                let rec = g.get(&parent).ok_or_else(BrowserError::missing)?;
                let dto = state_from_rec(app, rec);
                return Ok(dto);
            }
            Err(e) if e.code == "cdp_unsupported" => {}
            Err(e) => {
                tracing::debug!(
                    target: "zagens_browser",
                    code = %e.code,
                    "CDP history step failed; falling back to chrome history"
                );
            }
        }
    }

    let (mode, host_label) = {
        let mut g = hosts.lock()?;
        let rec = g.get_mut(&parent).ok_or_else(BrowserError::missing)?;
        if delta < 0 {
            if rec.history_idx == 0 {
                return Ok(state_from_rec(app, rec));
            }
            rec.history_idx -= 1;
        } else {
            if rec.history_idx + 1 >= rec.history.len() {
                return Ok(state_from_rec(app, rec));
            }
            rec.history_idx += 1;
        }
        rec.loading = true;
        (rec.mode, rec.host_label.clone())
    };
    let target = {
        let g = hosts.lock()?;
        let rec = g.get(&parent).ok_or_else(BrowserError::missing)?;
        rec.history.get(rec.history_idx).cloned()
    };
    if let Some(raw) = target {
        if let Ok(url) = Url::parse(&raw) {
            match mode {
                BrowserMode::Embedded => {
                    if let Some(wv) = app.get_webview(&host_label) {
                        let _ = wv.navigate(url);
                    }
                }
                BrowserMode::Windowed => {
                    if let Some(w) = app.get_webview_window(&host_label) {
                        let _ = w.navigate(url);
                    }
                }
            }
        } else {
            // Fallback to history API when URL is not parseable.
            let js = if delta < 0 {
                HISTORY_BACK_JS
            } else {
                HISTORY_FORWARD_JS
            };
            match mode {
                BrowserMode::Embedded => {
                    if let Some(wv) = app.get_webview(&host_label) {
                        let _ = wv.eval(js);
                    }
                }
                BrowserMode::Windowed => {
                    if let Some(w) = app.get_webview_window(&host_label) {
                        let _ = w.eval(js);
                    }
                }
            }
        }
    }
    let g = hosts.lock()?;
    let rec = g.get(&parent).ok_or_else(BrowserError::missing)?;
    let dto = state_from_rec(app, rec);
    emit_state(app, &parent, &dto);
    Ok(dto)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserPersistArgs {
    pub persist: bool,
}

#[tauri::command]
pub async fn browser_set_persist_profile(
    hosts: State<'_, BrowserHosts>,
    webview: tauri::Webview,
    args: BrowserPersistArgs,
) -> Result<BrowserPrefsDto, BrowserError> {
    let window = webview.window();
    if let Ok(mut g) = hosts.default_persist.lock() {
        *g = args.persist;
    }
    let parent = window.label().to_string();
    if let Ok(mut g) = hosts.lock()
        && let Some(rec) = g.get_mut(&parent)
    {
        rec.persist_profile = args.persist;
    }
    browser_get_prefs_inner(&hosts)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAllowHostArgs {
    pub host: Option<String>,
}

#[tauri::command]
pub async fn browser_allow_host(
    app: AppHandle,
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
    args: BrowserAllowHostArgs,
) -> Result<BrowserPrefsDto, BrowserError> {
    let window = webview.window();
    let host = if let Some(h) = args
        .host
        .map(|s| normalize_host(&s))
        .filter(|s| !s.is_empty())
    {
        h
    } else {
        let parent = window.label().to_string();
        let g = hosts.lock()?;
        let rec = g.get(&parent).ok_or_else(BrowserError::missing)?;
        let (url, _) = read_url_title(&app, rec);
        Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(normalize_host))
            .filter(|h| !h.is_empty())
            .ok_or_else(|| BrowserError::msg("no_host", "当前页没有可允许的主机名"))?
    };
    let _ = hosts.allow_host_name(&host)?;
    browser_get_prefs_inner(&hosts)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserPrefsArgs {
    pub persist_profile: Option<bool>,
    pub allow_private_lan: Option<bool>,
    pub yolo: Option<bool>,
}

#[tauri::command]
pub async fn browser_get_prefs(
    hosts: State<'_, BrowserHosts>,
) -> Result<BrowserPrefsDto, BrowserError> {
    browser_get_prefs_inner(&hosts)
}

#[tauri::command]
pub async fn browser_set_prefs(
    hosts: State<'_, BrowserHosts>,
    args: BrowserPrefsArgs,
) -> Result<BrowserPrefsDto, BrowserError> {
    if let Some(p) = args.persist_profile
        && let Ok(mut g) = hosts.default_persist.lock()
    {
        *g = p;
    }
    if let Some(lan) = args.allow_private_lan
        && let Ok(mut g) = hosts.allow_private_lan.lock()
    {
        *g = lan;
    }
    if let Some(yolo) = args.yolo
        && let Ok(mut g) = hosts.browser_yolo.lock()
    {
        *g = yolo;
    }
    if args.allow_private_lan.is_some() || args.yolo.is_some() {
        hosts.persist_prefs_disk();
    }
    browser_get_prefs_inner(&hosts)
}

fn browser_get_prefs_inner(hosts: &BrowserHosts) -> Result<BrowserPrefsDto, BrowserError> {
    let (allowlist, allow_private_lan) = hosts.nav_opts()?;
    Ok(BrowserPrefsDto {
        persist_profile: hosts.default_persist(),
        allow_private_lan,
        allowlist,
        yolo: hosts.browser_yolo(),
    })
}

pub(crate) fn lookup_host(
    hosts: &BrowserHosts,
    parent: &str,
) -> Result<(BrowserMode, String), BrowserError> {
    let g = hosts.lock()?;
    let rec = g.get(parent).ok_or_else(BrowserError::missing)?;
    if rec.creating {
        return Err(BrowserError::busy(parent));
    }
    Ok((rec.mode, rec.host_label.clone()))
}

pub(crate) async fn eval_js_string(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
    js: &str,
) -> Result<String, BrowserError> {
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let tx = std::sync::Mutex::new(Some(tx));
    let callback = move |result: String| {
        if let Ok(mut guard) = tx.lock()
            && let Some(sender) = guard.take()
        {
            let _ = sender.send(result);
        }
    };

    match mode {
        BrowserMode::Embedded => {
            let wv = app
                .get_webview(host_label)
                .ok_or_else(BrowserError::missing)?;
            wv.eval_with_callback(js, callback)
                .map_err(|e| BrowserError::msg("eval_failed", e.to_string()))?;
        }
        BrowserMode::Windowed => {
            let w = app
                .get_webview_window(host_label)
                .ok_or_else(BrowserError::missing)?;
            w.eval_with_callback(js, callback)
                .map_err(|e| BrowserError::msg("eval_failed", e.to_string()))?;
        }
    }

    let raw = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
        .await
        .map_err(|_| BrowserError::msg("eval_timeout", "eval 超时"))?
        .map_err(|_| BrowserError::msg("eval_canceled", "eval 通道关闭"))?;
    Ok(normalize_eval_json(&raw))
}

async fn eval_snapshot_on_host(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
) -> Result<BrowserSnapshotDto, BrowserError> {
    if cdp_snapshot::is_available() {
        match cdp_snapshot::snapshot_via_cdp(app, mode, host_label).await {
            Ok(snap) => return Ok(snap),
            Err(e) if e.code == "cdp_unsupported" => {}
            Err(e) => {
                tracing::debug!(
                    target: "zagens_browser",
                    code = %e.code,
                    "CDP snapshot failed; falling back to JS inject"
                );
            }
        }
    }
    let raw = eval_js_string(app, mode, host_label, &snapshot_js()).await?;
    Ok(parse_snapshot_json(&raw))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserWaitDto {
    pub ok: bool,
    pub kind: String,
    pub waited_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Poll until a page condition is met (B2). kind: `text` | `ref` | `selector` | `load`.
pub async fn agent_wait(
    app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
    kind: &str,
    value: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<BrowserWaitDto, BrowserError> {
    let kind = kind.trim().to_ascii_lowercase();
    if !matches!(kind.as_str(), "text" | "ref" | "selector" | "load") {
        return Err(BrowserError::msg(
            "bad_wait_kind",
            "wait kind 须为 text|ref|selector|load",
        ));
    }
    let value = value.unwrap_or("").to_string();
    if matches!(kind.as_str(), "text" | "ref" | "selector") && value.trim().is_empty() {
        return Err(BrowserError::msg(
            "missing_wait_value",
            format!("wait kind={kind} 需要 value（text / ref / selector）"),
        ));
    }
    let timeout_ms = timeout_ms.unwrap_or(8_000).clamp(200, 30_000);
    let (mode, host_label) = lookup_host(hosts, parent_label)?;
    let started = std::time::Instant::now();
    let script = wait_check_js(&kind, &value);
    loop {
        let raw = eval_js_string(app, mode, &host_label, &script).await?;
        #[derive(Deserialize)]
        struct WaitRaw {
            ok: Option<bool>,
            kind: Option<String>,
            detail: Option<String>,
        }
        let parsed: WaitRaw = serde_json::from_str(&raw).unwrap_or(WaitRaw {
            ok: Some(false),
            kind: Some(kind.clone()),
            detail: Some(raw.clone()),
        });
        if parsed.ok.unwrap_or(false) {
            return Ok(BrowserWaitDto {
                ok: true,
                kind: parsed.kind.unwrap_or_else(|| kind.clone()),
                waited_ms: started.elapsed().as_millis() as u64,
                detail: parsed.detail,
            });
        }
        if matches!(
            parsed.detail.as_deref(),
            Some("bad_kind") | Some("bad_selector")
        ) {
            return Err(BrowserError::msg(
                parsed.detail.unwrap_or_else(|| "wait_failed".into()),
                format!("browser_wait 条件无效: {raw}"),
            ));
        }
        if started.elapsed().as_millis() as u64 >= timeout_ms {
            return Err(BrowserError {
                code: "wait_timeout".into(),
                message: format!("browser_wait 超时（{timeout_ms}ms）kind={kind}"),
                hint: Some("可加大 timeout_ms，或先 browser_snapshot 确认页面状态".into()),
                detail: None,
            });
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
}

/// Agent/bridge navigate (NavActor::Agent). Does not create a host.
pub async fn agent_navigate(
    app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
    url_raw: &str,
) -> Result<BrowserStateDto, BrowserError> {
    let (allow, lan) = hosts.nav_opts()?;
    let ws = workspace_path_for(app, parent_label);
    let opts = NavOpts {
        allowlist: &allow,
        allow_private_lan: lan,
        workspace_root: ws.as_deref(),
    };
    let url = validate_navigation_with(url_raw, NavActor::Agent, &opts)
        .map_err(BrowserError::from_policy)?;
    let url_str = url.to_string();
    let (mode, host_label) = {
        let mut g = hosts.lock()?;
        let rec = g.get_mut(parent_label).ok_or_else(BrowserError::missing)?;
        if rec.creating {
            return Err(BrowserError::busy(parent_label));
        }
        rec.nav_policy.begin_agent();
        push_history(rec, &url_str);
        rec.loading = true;
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
    if let Some(host) = agent_external_https_host(&url_str) {
        let _ = hosts.allow_host_session(&host);
    }
    let g = hosts.lock()?;
    let rec = g.get(parent_label).ok_or_else(BrowserError::missing)?;
    let dto = state_from_rec(app, rec);
    emit_state(app, parent_label, &dto);
    Ok(dto)
}

pub async fn agent_snapshot(
    app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
    include_screenshot: bool,
) -> Result<BrowserSnapshotDto, BrowserError> {
    let (mode, host_label) = lookup_host(hosts, parent_label)?;
    let mut snap = eval_snapshot_on_host(app, mode, &host_label).await?;
    if include_screenshot {
        match screenshot::capture_screenshot_data_url(app, mode, &host_label).await {
            Ok(data_url) => {
                snap.screenshot = Some(data_url);
                snap.screenshot_note = None;
            }
            Err(e) => {
                snap.screenshot = None;
                snap.screenshot_note = Some(format!("{} ({})", e.message, e.code));
            }
        }
    }
    Ok(snap)
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
    let snap = agent_snapshot(app, hosts, parent_label, false).await?;
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
    app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
    limit: usize,
) -> Result<BrowserConsoleDto, BrowserError> {
    let (mode, host_label) = lookup_host(hosts, parent_label)?;
    let _ = eval_js_string(app, mode, &host_label, CONSOLE_HOOK_JS).await;
    let raw = eval_js_string(app, mode, &host_label, CONSOLE_TAIL_JS).await?;
    #[derive(Deserialize)]
    struct Line {
        level: Option<String>,
        message: Option<String>,
    }
    let parsed: Vec<Line> = serde_json::from_str(&raw).unwrap_or_default();
    let lines: Vec<String> = parsed
        .into_iter()
        .rev()
        .take(limit)
        .map(|l| {
            format!(
                "[{}] {}",
                l.level.unwrap_or_else(|| "log".into()),
                l.message.unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let note = if lines.is_empty() {
        "no console messages captured yet (hook installs on page load)".into()
    } else {
        String::new()
    };
    Ok(BrowserConsoleDto { lines, note })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creating_record(parent: &str) -> HostRecord {
        HostRecord {
            parent_label: parent.into(),
            host_label: String::new(),
            mode: BrowserMode::Embedded,
            visible: false,
            loading: true,
            persist_profile: false,
            history: Vec::new(),
            history_idx: 0,
            cdp_nav: None,
            creating: true,
            nav_policy: NavPolicyChain::default(),
        }
    }

    fn ready_record(parent: &str, host: &str) -> HostRecord {
        HostRecord {
            parent_label: parent.into(),
            host_label: host.into(),
            mode: BrowserMode::Embedded,
            visible: true,
            loading: false,
            persist_profile: true,
            history: vec!["about:blank".into()],
            history_idx: 0,
            cdp_nav: None,
            creating: false,
            nav_policy: NavPolicyChain::default(),
        }
    }

    /// T2: creating placeholder → busy, not missing.
    #[test]
    fn lookup_host_busy_while_creating() {
        let hosts = BrowserHosts::new();
        hosts
            .lock()
            .unwrap()
            .insert("win-a".into(), creating_record("win-a"));
        let err = lookup_host(&hosts, "win-a").unwrap_err();
        assert_eq!(err.code, "browser_creating");
        assert!(err.message.contains("win-a"));
    }

    /// T2: create-failure cleanup path — remove placeholder → missing.
    #[test]
    fn remove_creating_placeholder_yields_missing() {
        let hosts = BrowserHosts::new();
        {
            let mut g = hosts.lock().unwrap();
            g.insert("win-a".into(), creating_record("win-a"));
            g.remove("win-a"); // mirrors browser_create Err cleanup
        }
        let err = lookup_host(&hosts, "win-a").unwrap_err();
        assert_eq!(err.code, "browser_host_missing");
    }

    #[test]
    fn lookup_host_ok_when_ready() {
        let hosts = BrowserHosts::new();
        hosts
            .lock()
            .unwrap()
            .insert("win-a".into(), ready_record("win-a", "browser-embed-win-a"));
        let (mode, label) = lookup_host(&hosts, "win-a").unwrap();
        assert_eq!(mode, BrowserMode::Embedded);
        assert_eq!(label, "browser-embed-win-a");
        assert!(hosts.ready_host_parents().contains(&"win-a".into()));
    }

    #[test]
    fn allow_host_name_skips_loopback_and_normalizes() {
        let hosts = BrowserHosts::new();
        assert_eq!(hosts.allow_host_name("LocalHost").unwrap(), "localhost");
        let allowed = hosts.allow_host_name("Example.COM").unwrap();
        assert_eq!(allowed, "example.com");
        let (list, _) = hosts.nav_opts().unwrap();
        assert!(list.iter().any(|h| h == "example.com"));
        assert!(!list.iter().any(|h| h == "localhost"));
    }

    #[test]
    fn session_allowlist_does_not_touch_persistent_store() {
        let hosts = BrowserHosts::new();
        let before = hosts.persistent_allowlist.lock().unwrap().len();
        let _ = hosts.allow_host_session("session-only.example").unwrap();
        assert_eq!(hosts.persistent_allowlist.lock().unwrap().len(), before);
        let (merged, _) = hosts.nav_opts().unwrap();
        assert!(merged.iter().any(|h| h == "session-only.example"));
    }

    #[test]
    fn persistent_allowlist_writes_disk_field_only() {
        let hosts = BrowserHosts::new();
        let _ = hosts.allow_host_persistent("persist.example").unwrap();
        assert!(
            hosts
                .persistent_allowlist
                .lock()
                .unwrap()
                .contains("persist.example")
        );
    }

    #[test]
    fn busy_and_missing_error_codes_stable() {
        assert_eq!(BrowserError::missing().code, "browser_host_missing");
        assert_eq!(BrowserError::busy("p").code, "browser_creating");
        let policy = BrowserError::from_policy(url_policy::UrlPolicyError {
            code: "agent_external_needs_ask".into(),
            message: "ask".into(),
        });
        assert!(policy.hint.is_some());
    }
}
