//! Interactive PTY terminals for the DS Pick web UI (workspace-scoped shell).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

const MAX_SESSIONS: usize = 6;

pub struct TerminalManager {
    inner: Mutex<TerminalManagerInner>,
}

struct TerminalManagerInner {
    sessions: HashMap<String, LiveSession>,
}

struct LiveSession {
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
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("无法创建工作区目录: {e}"))?;
    }
    let canon = path
        .canonicalize()
        .map_err(|e| format!("工作区路径无效: {e}"))?;
    // PowerShell shows `FileSystem::\\?\F:\...` when cwd uses the Win32 verbatim prefix.
    Ok(PathBuf::from(
        crate::workspace_defaults::path_for_ui_display(canon),
    ))
}

fn shell_command(cwd: &Path) -> CommandBuilder {
    let mut cmd = build_shell_program();
    cmd.cwd(cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd
}

#[cfg(windows)]
fn executable_on_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
}

#[cfg(windows)]
fn windows_shell_exe() -> &'static str {
    static SHELL: OnceLock<String> = OnceLock::new();
    SHELL.get_or_init(|| {
        if executable_on_path("pwsh.exe") {
            "pwsh.exe".to_string()
        } else {
            "powershell.exe".to_string()
        }
    })
}

fn build_shell_program() -> CommandBuilder {
    #[cfg(windows)]
    {
        let mut c = CommandBuilder::new(windows_shell_exe());
        c.arg("-NoLogo");
        c.arg("-NoProfile");
        c
    }
    #[cfg(not(windows))]
    {
        if let Ok(shell) = std::env::var("SHELL") {
            let trimmed = shell.trim();
            if !trimmed.is_empty() {
                return CommandBuilder::new(trimmed);
            }
        }
        CommandBuilder::new("bash")
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

fn spawn_reader_thread(app: AppHandle, id: String, mut reader: Box<dyn Read + Send>) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let _ = app.emit(
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

#[tauri::command]
pub fn spawn_terminal(
    app: AppHandle,
    manager: State<'_, TerminalManager>,
    workspace: String,
    cols: u16,
    rows: u16,
) -> Result<String, String> {
    let cwd = resolve_terminal_cwd(&workspace)?;
    let id = Uuid::new_v4().to_string();

    let mut inner = manager
        .inner
        .lock()
        .map_err(|_| "终端管理器锁失败".to_string())?;

    if inner.sessions.len() >= MAX_SESSIONS {
        return Err(format!("最多同时打开 {MAX_SESSIONS} 个终端"));
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(pty_size(cols, rows))
        .map_err(|e| format!("无法创建 PTY: {e}"))?;

    let cmd = shell_command(&cwd);
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

    spawn_reader_thread(app.clone(), id.clone(), reader);

    let child = Arc::new(Mutex::new(child));
    let child_wait = Arc::clone(&child);
    let child_id = id.clone();
    let app_wait = app.clone();
    std::thread::spawn(move || {
        let code = match child_wait.lock() {
            Ok(mut c) => match c.wait() {
                Ok(status) => status.exit_code() as i32,
                Err(_) => -1,
            },
            Err(_) => -1,
        };
        let _ = app_wait.emit(
            "terminal-exit",
            TerminalExitPayload {
                id: child_id.clone(),
                code: Some(code),
            },
        );
        if let Some(mgr) = app_wait.try_state::<TerminalManager>() {
            if let Ok(mut inner) = mgr.inner.lock() {
                inner.sessions.remove(&child_id);
            }
        }
    });

    inner.sessions.insert(
        id.clone(),
        LiveSession {
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
    writer
        .flush()
        .map_err(|e| format!("刷新终端失败: {e}"))?;
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
    if let Some(session) = inner.sessions.remove(&id) {
        if let Ok(mut child) = session.child.lock() {
            let _ = child.kill();
        }
    }
    Ok(())
}
