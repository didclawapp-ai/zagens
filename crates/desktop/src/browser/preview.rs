//! `.zagens/preview.json` — start local preview server then open Browser pane.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use regex::Regex;
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
    /// When true, `ready_pattern` is treated as a Rust regex (C4).
    #[serde(default)]
    pub ready_regex: Option<bool>,
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
    #[serde(default)]
    pub timed_out: bool,
}

/// Per-parent preview child processes (C4).
pub struct PreviewProcess {
    inner: Mutex<HashMap<String, Child>>,
}

impl PreviewProcess {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    fn replace(&self, parent: &str, child: Child) {
        if let Ok(mut g) = self.inner.lock() {
            if let Some(mut prev) = g.remove(parent) {
                let _ = prev.kill();
                let _ = prev.wait();
            }
            g.insert(parent.to_string(), child);
        }
    }

    fn kill_for(&self, parent: &str) {
        if let Ok(mut g) = self.inner.lock()
            && let Some(mut prev) = g.remove(parent)
        {
            let _ = prev.kill();
            let _ = prev.wait();
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

fn line_matches(pattern: &str, line: &str, as_regex: bool) -> bool {
    if as_regex {
        Regex::new(pattern)
            .map(|re| re.is_match(line))
            .unwrap_or(false)
    } else {
        line.contains(pattern)
    }
}

/// Wait for ready_pattern. Drain threads keep reading after match to avoid pipe fill.
fn wait_ready(
    child: &mut Child,
    pattern: Option<&str>,
    as_regex: bool,
    timeout: Duration,
) -> (bool, Option<String>) {
    let Some(pat) = pattern.map(str::trim).filter(|s| !s.is_empty()) else {
        std::thread::sleep(Duration::from_millis(800));
        return (true, None);
    };
    if as_regex && Regex::new(pat).is_err() {
        return (false, Some(format!("invalid ready_pattern regex: {pat}")));
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    if let Some(out) = stdout {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(out);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
    }
    if let Some(err) = stderr {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(err);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
    }
    drop(tx);
    let deadline = Instant::now() + timeout;
    let mut matched: Option<String> = None;
    while Instant::now() < deadline {
        let remain = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remain.min(Duration::from_millis(200))) {
            Ok(line) => {
                if matched.is_none() && line_matches(pat, &line, as_regex) {
                    matched = Some(line);
                    // Keep draining briefly so the pipe does not fill; then return.
                    let drain_until = Instant::now() + Duration::from_millis(50);
                    while Instant::now() < drain_until {
                        match rx.try_recv() {
                            Ok(_) => {}
                            Err(_) => break,
                        }
                    }
                    return (true, matched);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    // Detached drain: keep consuming remaining lines in background (tx already dropped;
    // reader threads exit on EOF when child is killed).
    let _ = rx;
    (false, matched)
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

    preview_proc.kill_for(parent_label);
    let mut child = spawn_preview_command(&config.command, &cwd)?;
    let timeout = Duration::from_millis(config.ready_timeout_ms.unwrap_or(60_000).max(1_000));
    let as_regex = config.ready_regex.unwrap_or(false);
    let (ready, matched) = wait_ready(
        &mut child,
        config.ready_pattern.as_deref(),
        as_regex,
        timeout,
    );

    let timed_out = !ready;
    if timed_out {
        let _ = child.kill();
        let _ = child.wait();
        return Ok(PreviewStartResult {
            config,
            ready: false,
            matched_line: matched,
            browser: None,
            note: "preview ready_pattern 超时：已终止进程，未导航".into(),
            timed_out: true,
        });
    }

    preview_proc.replace(parent_label, child);

    let mut browser = None;
    let mut note = "preview ready".to_string();

    match agent_navigate(app, hosts, parent_label, &config.url).await {
        Ok(st) => browser = Some(st),
        Err(e) => {
            note = format!("{note}; navigate failed: {} ({})", e.message, e.code);
        }
    }

    Ok(PreviewStartResult {
        config,
        ready: true,
        matched_line: matched,
        browser,
        note,
        timed_out: false,
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
          "ready_pattern": "Local:",
          "readyRegex": false
        }"#;
        let cfg: PreviewConfig = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.command, "npm run dev");
        assert_eq!(cfg.url, "http://127.0.0.1:5173/");
        assert_eq!(cfg.ready_regex, Some(false));
    }

    #[test]
    fn line_matches_regex() {
        assert!(line_matches(
            r"Local:\s+http",
            "Local: http://127.0.0.1:5173/",
            true
        ));
        assert!(!line_matches(r"Local:\s+http", "ready", true));
        assert!(line_matches("Local:", ">> Local: ok", false));
    }
}
