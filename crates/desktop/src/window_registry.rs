//! Multi-window registry (Cursor / VS Code model): one process, many WebViews.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{
    AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder,
};
use uuid::Uuid;

pub const MAX_AGENT_WINDOWS: usize = 8;
const PRODUCT_TITLE: &str = "Zagens";

pub struct WindowRegistry {
    inner: Mutex<RegistryInner>,
}

struct RegistryInner {
    windows: HashMap<String, WindowRecord>,
    thread_owner: HashMap<String, String>,
    last_focused: String,
}

#[derive(Clone)]
struct WindowRecord {
    label: String,
    primary_workspace: String,
    focused_thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWindowSummary {
    pub label: String,
    pub primary_workspace: String,
    pub title: String,
}

impl WindowRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(RegistryInner {
                windows: HashMap::new(),
                thread_owner: HashMap::new(),
                last_focused: "main".to_string(),
            }),
        }
    }

    fn lock_inner(&self) -> Result<std::sync::MutexGuard<'_, RegistryInner>, String> {
        self.inner
            .lock()
            .map_err(|_| "窗口注册表锁失败".to_string())
    }

    pub fn register(
        &self,
        label: impl Into<String>,
        primary_workspace: impl Into<String>,
    ) -> Result<(), String> {
        let label = label.into();
        let primary_workspace = normalize_workspace_field(primary_workspace.into())?;
        let mut g = self.lock_inner()?;
        g.windows.insert(
            label.clone(),
            WindowRecord {
                label,
                primary_workspace,
                focused_thread_id: None,
            },
        );
        Ok(())
    }

    pub fn unregister(&self, label: &str) {
        if let Ok(mut g) = self.inner.lock() {
            g.windows.remove(label);
            g.thread_owner.retain(|_, owner| owner != label);
            if g.last_focused == label {
                g.last_focused = g
                    .windows
                    .keys()
                    .next()
                    .cloned()
                    .unwrap_or_else(|| "main".to_string());
            }
        }
    }

    pub fn window_count(&self) -> usize {
        self.inner.lock().map(|g| g.windows.len()).unwrap_or(0)
    }

    pub fn set_last_focused(&self, label: &str) {
        if let Ok(mut g) = self.inner.lock() {
            if g.windows.contains_key(label) {
                g.last_focused = label.to_string();
            }
        }
    }

    pub fn last_focused_label(&self) -> String {
        self.inner
            .lock()
            .map(|g| g.last_focused.clone())
            .unwrap_or_else(|_| "main".to_string())
    }

    pub fn primary_workspace(&self, label: &str) -> Option<String> {
        self.inner
            .lock()
            .ok()
            .and_then(|g| g.windows.get(label).map(|w| w.primary_workspace.clone()))
    }

    pub fn register_thread(&self, window_label: &str, thread_id: &str) -> Result<(), String> {
        let tid = thread_id.trim();
        if tid.is_empty() {
            return Err("thread_id 无效".to_string());
        }
        let mut g = self.lock_inner()?;
        if !g.windows.contains_key(window_label) {
            return Err(format!("未知窗口: {window_label}"));
        }
        g.thread_owner.insert(tid.to_string(), window_label.to_string());
        if let Some(rec) = g.windows.get_mut(window_label) {
            rec.focused_thread_id = Some(tid.to_string());
        }
        Ok(())
    }

    pub fn thread_owner_label(&self, thread_id: &str) -> Option<String> {
        self.inner
            .lock()
            .ok()
            .and_then(|g| g.thread_owner.get(thread_id).cloned())
    }

    pub fn list_summaries(&self) -> Result<Vec<AgentWindowSummary>, String> {
        let g = self.lock_inner()?;
        let mut out: Vec<AgentWindowSummary> = g
            .windows
            .values()
            .map(|w| AgentWindowSummary {
                label: w.label.clone(),
                primary_workspace: w.primary_workspace.clone(),
                title: window_title_for_workspace(&w.primary_workspace),
            })
            .collect();
        out.sort_by(|a, b| a.label.cmp(&b.label));
        Ok(out)
    }
}

fn normalize_workspace_field(raw: String) -> Result<String, String> {
    let t = raw.trim();
    if t.is_empty() {
        return crate::workspace_defaults::default_composer_workspace();
    }
    let p = PathBuf::from(t);
    if p.exists() {
        let canon = p
            .canonicalize()
            .map_err(|e| format!("工作区路径无效: {e}"))?;
        if !canon.is_dir() {
            return Err("工作区必须是目录".to_string());
        }
        return Ok(crate::workspace_defaults::path_for_ui_display(canon));
    }
    std::fs::create_dir_all(&p).map_err(|e| format!("无法创建工作区: {e}"))?;
    let canon = p
        .canonicalize()
        .map_err(|e| format!("工作区路径无效: {e}"))?;
    Ok(crate::workspace_defaults::path_for_ui_display(canon))
}

pub fn window_title_for_workspace(workspace: &str) -> String {
    let ws = workspace.trim();
    if ws.is_empty() {
        return PRODUCT_TITLE.to_string();
    }
    let name = Path::new(ws)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(ws);
    format!("{name} — {PRODUCT_TITLE}")
}

fn webview_url() -> WebviewUrl {
    if cfg!(debug_assertions) {
        if let Ok(url) = "http://localhost:1420".parse() {
            return WebviewUrl::External(url);
        }
    }
    WebviewUrl::App("index.html".into())
}

pub fn build_agent_window(
    app: &AppHandle,
    label: &str,
    workspace: &str,
) -> Result<tauri::WebviewWindow, String> {
    let title = window_title_for_workspace(workspace);
    WebviewWindowBuilder::new(app, label, webview_url())
        .title(title)
        .inner_size(1200.0, 800.0)
        .min_inner_size(800.0, 600.0)
        .decorations(false)
        .center()
        .visible(true)
        .build()
        .map_err(|e| format!("创建窗口失败: {e}"))
}

pub fn focus_window(app: &AppHandle, label: &str) -> Result<(), String> {
    let w = app
        .get_webview_window(label)
        .ok_or_else(|| format!("窗口不存在: {label}"))?;
    w.show().map_err(|e| e.to_string())?;
    w.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

/// Second instance or tray: open/focus a window for `workspace` (optional).
pub fn open_or_focus_workspace(app: &AppHandle, workspace: Option<String>) -> Result<String, String> {
    let registry = app.state::<WindowRegistry>();
    let ws = match workspace {
        Some(s) if !s.trim().is_empty() => normalize_workspace_field(s)?,
        _ => crate::workspace_defaults::default_composer_workspace()?,
    };
    let existing_label = registry.inner.lock().ok().and_then(|guard| {
        guard
            .windows
            .values()
            .find(|rec| paths_equal(&rec.primary_workspace, &ws))
            .map(|rec| rec.label.clone())
    });
    if let Some(label) = existing_label {
        focus_window(app, &label)?;
        registry.set_last_focused(&label);
        return Ok(label);
    }
    create_agent_window_impl(app, Some(ws))
}

fn paths_equal(a: &str, b: &str) -> bool {
    let pa = PathBuf::from(a.trim());
    let pb = PathBuf::from(b.trim());
    pa.canonicalize()
        .ok()
        .zip(pb.canonicalize().ok())
        .map(|(ca, cb)| ca == cb)
        .unwrap_or_else(|| a.trim().eq_ignore_ascii_case(b.trim()))
}

pub fn create_agent_window_impl(
    app: &AppHandle,
    workspace: Option<String>,
) -> Result<String, String> {
    let registry = app.state::<WindowRegistry>();
    if registry.window_count() >= MAX_AGENT_WINDOWS {
        return Err(format!("最多同时打开 {MAX_AGENT_WINDOWS} 个窗口"));
    }

    let ws = match workspace {
        Some(s) => normalize_workspace_field(s)?,
        None => {
            if let Some(main) = registry.primary_workspace("main") {
                main
            } else {
                crate::workspace_defaults::default_composer_workspace()?
            }
        }
    };

    let label = format!("pick-{}", Uuid::new_v4());

    let window = build_agent_window(app, &label, &ws)?;
    registry.register(&label, &ws)?;
    registry.set_last_focused(&label);
    let _ = window.set_focus();
    Ok(label)
}

#[tauri::command]
pub fn get_window_label(window: tauri::WebviewWindow) -> String {
    window.label().to_string()
}

#[tauri::command]
pub fn get_window_workspace(
    window: tauri::WebviewWindow,
    registry: State<'_, WindowRegistry>,
) -> Result<String, String> {
    let label = window.label().to_string();
    registry
        .primary_workspace(&label)
        .ok_or_else(|| "窗口未注册工作区".to_string())
}

#[tauri::command]
pub fn create_agent_window(
    app: AppHandle,
    workspace: Option<String>,
) -> Result<String, String> {
    create_agent_window_impl(&app, workspace)
}

#[tauri::command]
pub fn list_agent_windows(registry: State<'_, WindowRegistry>) -> Result<Vec<AgentWindowSummary>, String> {
    registry.list_summaries()
}

#[tauri::command]
pub fn focus_agent_window(app: AppHandle, label: String) -> Result<(), String> {
    focus_window(&app, &label)?;
    app.state::<WindowRegistry>().set_last_focused(&label);
    Ok(())
}

#[tauri::command]
pub fn register_window_thread(
    window: tauri::WebviewWindow,
    registry: State<'_, WindowRegistry>,
    thread_id: String,
) -> Result<(), String> {
    registry.register_thread(window.label(), &thread_id)
}

#[tauri::command]
pub fn thread_owned_by_window(
    window: tauri::WebviewWindow,
    registry: State<'_, WindowRegistry>,
    thread_id: String,
) -> bool {
    registry
        .thread_owner_label(&thread_id)
        .is_some_and(|owner| owner == window.label())
}

#[tauri::command]
pub fn close_current_window(
    window: tauri::WebviewWindow,
    registry: State<'_, WindowRegistry>,
    terminal: State<'_, crate::terminal::TerminalManager>,
) -> Result<(), String> {
    let label = window.label().to_string();
    let count = registry.window_count();
    if count <= 1 {
        let _ = window.hide();
        return Ok(());
    }
    terminal.kill_all_for_window(&label);
    registry.unregister(&label);
    window.close().map_err(|e| e.to_string())
}

pub fn handle_close_requested(
    window: &tauri::WebviewWindow,
    api: &tauri::CloseRequestApi,
    registry: &WindowRegistry,
    terminal: &crate::terminal::TerminalManager,
) {
    let label = window.label().to_string();
    let count = registry.window_count();
    if count <= 1 {
        api.prevent_close();
        let _ = window.hide();
        return;
    }
    terminal.kill_all_for_window(&label);
    registry.unregister(&label);
}

pub fn parse_workspace_from_args(args: &[String]) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--workspace" || arg == "-w" {
            if let Some(path) = iter.next() {
                return Some(path.clone());
            }
        }
    }
    args.iter()
        .find(|a| !a.starts_with('-') && Path::new(a.as_str()).is_absolute())
        .cloned()
}
