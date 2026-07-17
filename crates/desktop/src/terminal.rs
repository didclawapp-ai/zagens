//! Interactive PTY terminals for the Zagens web UI (workspace-scoped shell).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

/// Per-window PTY cap (see docs/desktop/multi-window-plan.md).
const MAX_SESSIONS_PER_WINDOW: usize = 4;
const MAX_SESSIONS_GLOBAL: usize = 16;

pub struct TerminalManager {
    inner: Mutex<TerminalManagerInner>,
}

struct TerminalManagerInner {
    sessions: HashMap<String, LiveSession>,
}

struct LiveSession {
    window_label: String,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self {
            inner: Mutex::new(TerminalManagerInner {
                sessions: HashMap::new(),
            }),
        }
    }
}

impl TerminalManager {
    pub fn kill_all_for_window(&self, window_label: &str) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let ids: Vec<String> = inner
            .sessions
            .iter()
            .filter(|(_, s)| s.window_label == window_label)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(session) = inner.sessions.remove(&id)
                && let Ok(mut child) = session.child.lock()
            {
                let _ = child.kill();
            }
        }
    }
}

#[derive(Clone, Serialize)]
struct TerminalDataPayload {
    id: String,
    data: String,
}

#[derive(Clone, Serialize)]
struct TerminalExitPayload {
    id: String,
    code: Option<i32>,
}

fn resolve_terminal_cwd(workspace: &str) -> Result<PathBuf, String> {
    let trimmed = workspace.trim();
    let path = if trimmed.is_empty() {
        PathBuf::from(crate::workspace_defaults::default_composer_workspace()?)
    } else {
        PathBuf::from(trimmed)
    };
    if !path.exists() {
        std::fs::create_dir_all(&path).map_err(|e| format!("无法创建工作区目录: {e}"))?;
    }
    let canon = path
        .canonicalize()
        .map_err(|e| format!("工作区路径无效: {e}"))?;
    Ok(PathBuf::from(
        crate::workspace_defaults::path_for_ui_display(canon),
    ))
}

fn shell_command(cwd: &Path, shell: &str, load_profile: bool) -> Result<CommandBuilder, String> {
    let mut cmd = build_shell_program(shell, load_profile)?;
    cmd.cwd(cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // Common CLIs (npm, cargo, etc.) gate ANSI on these when attached to a PTY.
    cmd.env("FORCE_COLOR", "1");
    cmd.env("CLICOLOR_FORCE", "1");
    cmd.env("npm_config_color", "always");
    Ok(cmd)
}

#[cfg(windows)]
fn executable_on_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
}

#[cfg(windows)]
fn windows_default_shell_exe() -> &'static str {
    use std::sync::OnceLock;
    static SHELL: OnceLock<String> = OnceLock::new();
    SHELL.get_or_init(|| {
        if executable_on_path("pwsh.exe") {
            "pwsh.exe".to_string()
        } else {
            "powershell.exe".to_string()
        }
    })
}

#[cfg(windows)]
fn build_powershell(exe: &str, load_profile: bool) -> CommandBuilder {
    let is_pwsh = exe.eq_ignore_ascii_case("pwsh.exe");
    let mut c = CommandBuilder::new(exe);
    c.arg("-NoLogo");
    if !load_profile {
        c.arg("-NoProfile");
    }
    // Prefer ANSI even when profile is skipped / does not set PSStyle.
    if is_pwsh {
        c.arg("-NoExit");
        c.arg("-Command");
        c.arg("$PSStyle.OutputRendering = 'Ansi'");
    }
    c
}

fn build_shell_program(shell: &str, load_profile: bool) -> Result<CommandBuilder, String> {
    let kind = shell.trim().to_ascii_lowercase();
    let kind = if kind.is_empty() {
        "default"
    } else {
        kind.as_str()
    };

    #[cfg(windows)]
    {
        match kind {
            "default" => Ok(build_powershell(windows_default_shell_exe(), load_profile)),
            "pwsh" => {
                if !executable_on_path("pwsh.exe") {
                    return Err("未找到 pwsh.exe（PowerShell 7+）".to_string());
                }
                Ok(build_powershell("pwsh.exe", load_profile))
            }
            "powershell" => Ok(build_powershell("powershell.exe", load_profile)),
            "cmd" => Ok(CommandBuilder::new("cmd.exe")),
            other => Err(format!("不支持的 Shell: {other}")),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = load_profile; // POSIX login shells use profile via the user's shell; no -NoProfile analog here.
        match kind {
            "default" => {
                if let Ok(shell) = std::env::var("SHELL") {
                    let trimmed = shell.trim();
                    if !trimmed.is_empty() {
                        return Ok(CommandBuilder::new(trimmed));
                    }
                }
                Ok(CommandBuilder::new("bash"))
            }
            "bash" => Ok(CommandBuilder::new("bash")),
            "zsh" => Ok(CommandBuilder::new("zsh")),
            "sh" => Ok(CommandBuilder::new("sh")),
            other => Err(format!("unsupported shell: {other}")),
        }
    }
}

fn pty_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(2),
        cols: cols.max(4),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn spawn_reader_thread(
    app: AppHandle,
    window_label: String,
    id: String,
    mut reader: Box<dyn Read + Send>,
) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let _ = app.emit_to(
                        &window_label,
                        "terminal-data",
                        TerminalDataPayload {
                            id: id.clone(),
                            data: chunk,
                        },
                    );
                }
                Err(_) => break,
            }
        }
    });
}

/// Spawn an interactive PTY shell for the workspace panel.
///
/// `shell`: `default` | `pwsh` | `powershell` | `cmd` (Windows) / `bash` | `zsh` | `sh` (Unix).
/// `load_profile`: when true, PowerShell loads the user profile (omits `-NoProfile`). Ignored on Unix.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn spawn_terminal(
    webview: tauri::Webview,
    app: AppHandle,
    manager: State<'_, TerminalManager>,
    workspace: String,
    cols: u16,
    rows: u16,
    shell: Option<String>,
    load_profile: Option<bool>,
) -> Result<String, String> {
    let window_label = webview.window().label().to_string();
    let cwd = resolve_terminal_cwd(&workspace)?;
    let id = Uuid::new_v4().to_string();
    let shell_kind = shell.unwrap_or_else(|| "default".to_string());
    let load_profile = load_profile.unwrap_or(false);

    let mut inner = manager
        .inner
        .lock()
        .map_err(|_| "终端管理器锁失败".to_string())?;

    if inner.sessions.len() >= MAX_SESSIONS_GLOBAL {
        return Err(format!("最多同时打开 {MAX_SESSIONS_GLOBAL} 个终端"));
    }
    let per_window = inner
        .sessions
        .values()
        .filter(|s| s.window_label == window_label)
        .count();
    if per_window >= MAX_SESSIONS_PER_WINDOW {
        return Err(format!(
            "本窗口最多同时打开 {MAX_SESSIONS_PER_WINDOW} 个终端"
        ));
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(pty_size(cols, rows))
        .map_err(|e| format!("无法创建 PTY: {e}"))?;

    let cmd = shell_command(&cwd, &shell_kind, load_profile)?;
    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("无法启动 Shell: {e}"))?;

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("无法读取终端: {e}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("无法写入终端: {e}"))?;

    let master = Arc::new(Mutex::new(pair.master));
    let writer = Arc::new(Mutex::new(writer));

    spawn_reader_thread(app.clone(), window_label.clone(), id.clone(), reader);

    let child = Arc::new(Mutex::new(child));
    let child_wait = Arc::clone(&child);
    let child_id = id.clone();
    let app_wait = app.clone();
    let win_for_exit = window_label.clone();
    std::thread::spawn(move || {
        let code = match child_wait.lock() {
            Ok(mut c) => match c.wait() {
                Ok(status) => status.exit_code() as i32,
                Err(_) => -1,
            },
            Err(_) => -1,
        };
        let _ = app_wait.emit_to(
            &win_for_exit,
            "terminal-exit",
            TerminalExitPayload {
                id: child_id.clone(),
                code: Some(code),
            },
        );
        if let Some(mgr) = app_wait.try_state::<TerminalManager>()
            && let Ok(mut inner) = mgr.inner.lock()
        {
            inner.sessions.remove(&child_id);
        }
    });

    inner.sessions.insert(
        id.clone(),
        LiveSession {
            window_label,
            master,
            writer,
            child,
        },
    );

    Ok(id)
}

#[tauri::command]
pub fn write_terminal(
    manager: State<'_, TerminalManager>,
    id: String,
    data: String,
) -> Result<(), String> {
    let inner = manager
        .inner
        .lock()
        .map_err(|_| "终端管理器锁失败".to_string())?;
    let session = inner
        .sessions
        .get(&id)
        .ok_or_else(|| "终端会话不存在或已结束".to_string())?;
    let mut writer = session
        .writer
        .lock()
        .map_err(|_| "终端写入锁失败".to_string())?;
    writer
        .write_all(data.as_bytes())
        .map_err(|e| format!("写入终端失败: {e}"))?;
    writer.flush().map_err(|e| format!("刷新终端失败: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn resize_terminal(
    manager: State<'_, TerminalManager>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let inner = manager
        .inner
        .lock()
        .map_err(|_| "终端管理器锁失败".to_string())?;
    let session = inner
        .sessions
        .get(&id)
        .ok_or_else(|| "终端会话不存在或已结束".to_string())?;
    let master = session
        .master
        .lock()
        .map_err(|_| "终端锁失败".to_string())?;
    master
        .resize(pty_size(cols, rows))
        .map_err(|e| format!("调整终端大小失败: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn kill_terminal(manager: State<'_, TerminalManager>, id: String) -> Result<(), String> {
    let mut inner = manager
        .inner
        .lock()
        .map_err(|_| "终端管理器锁失败".to_string())?;
    if let Some(session) = inner.sessions.remove(&id)
        && let Ok(mut child) = session.child.lock()
    {
        let _ = child.kill();
    }
    Ok(())
}
