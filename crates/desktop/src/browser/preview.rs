//! `.zagens/preview.json` — start local preview server then open Browser pane.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use super::url_policy::NavOpts;
use super::{BrowserError, BrowserHosts, BrowserStateDto, agent_navigate, validate_human_url};
use crate::window_registry::WindowRegistry;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewConfig {
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
    pub url: String,
    #[serde(default)]
    pub ready_pattern: Option<String>,
    #[serde(default)]
    pub ready_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewStartResult {
    pub config: PreviewConfig,
    pub ready: bool,
    pub matched_line: Option<String>,
    pub browser: Option<BrowserStateDto>,
    pub note: String,
}

pub struct PreviewProcess {
    inner: Mutex<Option<Child>>,
}

impl PreviewProcess {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    fn replace(&self, child: Child) {
        if let Ok(mut g) = self.inner.lock() {
            if let Some(mut prev) = g.take() {
                let _ = prev.kill();
                let _ = prev.wait();
            }
            *g = Some(child);
        }
    }
}

fn preview_path(workspace: &Path) -> PathBuf {
    workspace.join(".zagens").join("preview.json")
}

pub fn read_preview_config(workspace: &Path) -> Result<PreviewConfig, BrowserError> {
    let path = preview_path(workspace);
    let raw = std::fs::read_to_string(&path).map_err(|e| {
        BrowserError::msg("preview_missing", format!("找不到 {}: {e}", path.display()))
    })?;
    serde_json::from_str(&raw)
        .map_err(|e| BrowserError::msg("preview_invalid", format!("preview.json 解析失败: {e}")))
}

fn resolve_cwd(workspace: &Path, cwd: Option<&str>) -> Result<PathBuf, BrowserError> {
    let raw = cwd.map(str::trim).filter(|s| !s.is_empty()).unwrap_or(".");
    let joined = if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        workspace.join(raw)
    };
    let canon = std::fs::canonicalize(&joined).map_err(|e| {
        BrowserError::msg(
            "cwd_invalid",
            format!("无法解析 cwd {}: {e}", joined.display()),
        )
    })?;
    let ws = std::fs::canonicalize(workspace)
        .map_err(|e| BrowserError::msg("workspace_invalid", format!("无法解析 workspace: {e}")))?;
    if !canon.starts_with(&ws) {
        return Err(BrowserError::msg(
            "cwd_escape",
            "preview cwd 必须位于 workspace 内",
        ));
    }
    Ok(canon)
}

fn spawn_preview_command(command: &str, cwd: &Path) -> Result<Child, BrowserError> {
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args(["/C", command])
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| BrowserError::msg("preview_spawn", format!("启动 preview 失败: {e}")))
    }
    #[cfg(not(windows))]
    {
        Command::new("sh")
            .args(["-lc", command])
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| BrowserError::msg("preview_spawn", format!("启动 preview 失败: {e}")))
    }
}

/// `ready_pattern` is a case-sensitive substring (keeps deps light).
fn line_matches(pattern: &str, line: &str) -> bool {
    line.contains(pattern)
}

fn wait_ready(
    child: &mut Child,
    pattern: Option<&str>,
    timeout: Duration,
) -> (bool, Option<String>) {
    let Some(pat) = pattern.map(str::trim).filter(|s| !s.is_empty()) else {
        std::thread::sleep(Duration::from_millis(800));
        return (true, None);
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    if let Some(out) = stdout {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(out);
            for line in reader.lines().flatten() {
                let _ = tx.send(line);
            }
        });
    }
    if let Some(err) = stderr {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(err);
            for line in reader.lines().flatten() {
                let _ = tx.send(line);
            }
        });
    }
    drop(tx);
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let remain = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remain.min(Duration::from_millis(200))) {
            Ok(line) => {
                if line_matches(pat, &line) {
                    return (true, Some(line));
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    (false, None)
}

async fn start_preview_inner(
    app: &AppHandle,
    hosts: &BrowserHosts,
    preview_proc: &PreviewProcess,
    parent_label: &str,
    workspace: &Path,
) -> Result<PreviewStartResult, BrowserError> {
    let config = read_preview_config(workspace)?;
    let cwd = resolve_cwd(workspace, config.cwd.as_deref())?;
    let (allow, lan) = hosts.nav_opts()?;
    let opts = NavOpts {
        allowlist: &allow,
        allow_private_lan: lan,
        workspace_root: Some(workspace),
    };
    let _url = validate_human_url(&config.url, &opts).map_err(BrowserError::from_policy)?;

    let mut child = spawn_preview_command(&config.command, &cwd)?;
    let timeout = Duration::from_millis(config.ready_timeout_ms.unwrap_or(60_000).max(1_000));
    let (ready, matched) = wait_ready(&mut child, config.ready_pattern.as_deref(), timeout);
    preview_proc.replace(child);

    let mut browser = None;
    let mut note = if ready {
        "preview ready".into()
    } else {
        "preview started but ready_pattern 未匹配（仍尝试打开 URL）".into()
    };

    match agent_navigate(app, hosts, parent_label, &config.url).await {
        Ok(st) => browser = Some(st),
        Err(e) => {
            note = format!("{note}; navigate failed: {} ({})", e.message, e.code);
        }
    }

    Ok(PreviewStartResult {
        config,
        ready,
        matched_line: matched,
        browser,
        note,
    })
}

#[tauri::command]
pub async fn browser_preview_get(
    app: AppHandle,
    webview: tauri::Webview,
) -> Result<PreviewConfig, BrowserError> {
    let registry = app.state::<WindowRegistry>();
    let ws = registry
        .primary_workspace(webview.window().label())
        .ok_or_else(|| BrowserError::msg("no_workspace", "当前窗没有 workspace"))?;
    read_preview_config(Path::new(&ws))
}

#[tauri::command]
pub async fn browser_preview_start(
    app: AppHandle,
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
    preview_proc: State<'_, PreviewProcess>,
) -> Result<PreviewStartResult, BrowserError> {
    let registry = app.state::<WindowRegistry>();
    let ws = registry
        .primary_workspace(webview.window().label())
        .ok_or_else(|| BrowserError::msg("no_workspace", "当前窗没有 workspace"))?;
    start_preview_inner(
        &app,
        &hosts,
        &preview_proc,
        webview.window().label(),
        Path::new(&ws),
    )
    .await
}

pub async fn agent_start_preview(
    app: &AppHandle,
    hosts: &BrowserHosts,
    preview_proc: &PreviewProcess,
    parent_label: &str,
    workspace: Option<&str>,
) -> Result<PreviewStartResult, BrowserError> {
    let ws = if let Some(w) = workspace.map(str::trim).filter(|s| !s.is_empty()) {
        w.to_string()
    } else {
        let registry = app.state::<WindowRegistry>();
        registry
            .primary_workspace(parent_label)
            .ok_or_else(|| BrowserError::msg("no_workspace", "无法解析 workspace"))?
    };
    start_preview_inner(app, hosts, preview_proc, parent_label, Path::new(&ws)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_preview_json() {
        let raw = r#"{
          "command": "npm run dev",
          "cwd": ".",
          "url": "http://127.0.0.1:5173/",
          "ready_pattern": "Local:"
        }"#;
        let cfg: PreviewConfig = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.command, "npm run dev");
        assert_eq!(cfg.url, "http://127.0.0.1:5173/");
    }
}
