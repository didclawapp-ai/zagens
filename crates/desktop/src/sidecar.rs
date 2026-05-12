use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::StatusCode;
use tauri::{AppHandle, Manager};
use tokio::process::Command;
use tokio::sync::Notify;
use tokio::time::sleep;

const HEALTH_CHECK_INTERVAL_SECS: u64 = 5;
/// After spawning the sidecar, poll quickly so we detect `/health` as soon as it binds (1s was
/// a fixed blind wait that added up to ~1s latency on fast starts and aligned poorly with UI polls).
const MAX_STARTUP_RETRIES: u32 = 60;
const STARTUP_FIRST_DELAY_MS: u64 = 60;
const STARTUP_RETRY_DELAY_MS: u64 = 200;
const MAX_HEALTH_FAILURES: u32 = 3;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Shared client for localhost probes (avoids reconstructing TLS stacks per request on Windows).
static SIDECAR_PROBE_HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(400))
        .timeout(Duration::from_secs(2))
        .build()
        .expect("sidecar probe reqwest client")
});

/// Avoid inheriting System32/System as the implicit cwd for embedded `deepseek serve`:
/// tooling defaults (and broken session resumes) would otherwise latch onto that directory.
fn sidecar_spawn_cwd() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        return std::env::var_os("USERPROFILE").map(PathBuf::from);
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// Path to the sidecar stderr log file. Created under `~/.deepseek/logs/` on
/// first spawn — the parent directory is ensured before opening the file (#H4).
fn sidecar_stderr_log_path() -> Option<PathBuf> {
    let home = sidecar_spawn_cwd()?;
    let log_dir = home.join(".deepseek").join("logs");
    std::fs::create_dir_all(&log_dir).ok()?;
    Some(log_dir.join("sidecar.log"))
}

/// Read the last `max_lines` lines from a text file, best-effort.
fn read_log_tail(path: &Path, max_lines: usize) -> String {
    let Ok(data) = std::fs::read_to_string(path) else {
        return "(log unreadable)".to_string();
    };
    let lines: Vec<&str> = data.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

fn spawn_sidecar(deepseek_bin: &str, port: u16, token: &str) -> Result<Command> {
    let port_s = port.to_string();
    let mut std_cmd = std::process::Command::new(deepseek_bin);
    // Pass the auth token via environment variable (DEEPSEEK_RUNTIME_TOKEN)
    // instead of --auth-token CLI arg to keep it out of `ps` / process lists
    // on all platforms (#H5). The serve binary reads this env as a fallback
    // for RuntimeApiOptions::auth_token.
    std_cmd.env("DEEPSEEK_RUNTIME_TOKEN", token);
    // Lets the shared runtime tune system prompts (e.g. DS Pick vs terminal TUI).
    std_cmd.env("DEEPSEEK_CLIENT_SURFACE", "ds-pick");
    std_cmd
        .args([
            "serve",
            "--http",
            "--host",
            "127.0.0.1",
            "--port",
            port_s.as_str(),
            "--cors-origin",
            "http://tauri.localhost",
            "--cors-origin",
            "https://tauri.localhost",
        ])
        .stdin(Stdio::null());
    if let Some(log_path) = sidecar_stderr_log_path() {
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap_or_else(|_| std::fs::File::create(&log_path).unwrap());
        // `deepseek serve` logs "Runtime API listening" to stdout; merge both streams into the
        // same file so `~/.deepseek/logs/sidecar.log` is actually useful on Windows.
        match log_file.try_clone() {
            Ok(stderr_dup) => {
                std_cmd.stdout(Stdio::from(log_file));
                std_cmd.stderr(Stdio::from(stderr_dup));
            }
            Err(_) => {
                std_cmd.stdout(Stdio::null());
                std_cmd.stderr(Stdio::from(log_file));
            }
        }
    } else {
        std_cmd.stdout(Stdio::null());
        std_cmd.stderr(Stdio::null());
    }
    if let Some(cwd) = sidecar_spawn_cwd()
        && cwd.is_dir()
    {
        std_cmd.current_dir(cwd);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std_cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut cmd = Command::from(std_cmd);
    cmd.kill_on_drop(true);
    Ok(cmd)
}

async fn is_healthy(port: u16, _token: &str) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");
    match SIDECAR_PROBE_HTTP.get(&url).send().await {
        Ok(resp) => resp.status() == StatusCode::OK,
        Err(_) => false,
    }
}

/// Both `/health` and a token-authenticated `/v1/sessions` probe succeed (runs probes concurrently).
async fn sidecar_ready(port: u16, token: &str) -> bool {
    let (health_ok, api_ok) = tokio::join!(
        is_healthy(port, token),
        runtime_api_accepts_token(port, token)
    );
    health_ok && api_ok
}

/// True when `/v1/*` accepts this install's bearer token (not only `/health`).
async fn runtime_api_accepts_token(port: u16, token: &str) -> bool {
    if token.trim().is_empty() {
        return true;
    }
    let url = format!("http://127.0.0.1:{port}/v1/sessions");
    match SIDECAR_PROBE_HTTP
        .get(&url)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
    {
        Ok(resp) => resp.status() != StatusCode::UNAUTHORIZED,
        Err(_) => false,
    }
}

/// Best-effort: stop processes listening on loopback `port` so we can bind a new sidecar.
/// Used when an old `deepseek-tui serve` is still bound after the desktop app restarted
/// (new random auth token → `/health` OK but `/v1/*` returns 401).
fn kill_processes_listening_on_local_port(port: u16) -> Result<()> {
    #[cfg(windows)]
    {
        let script = format!(
            r#"$port = {}; Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue | Where-Object {{ $_.LocalAddress -eq '127.0.0.1' -or $_.LocalAddress -eq '::1' }} | ForEach-Object {{ Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }}"#,
            port
        );
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .status()
            .context("failed to run PowerShell to reclaim runtime port")?;
    }
    #[cfg(unix)]
    {
        use std::process::Command as StdCommand;
        let itcp = format!("-iTCP:{port}");
        if let Ok(out) = StdCommand::new("lsof")
            .args(["-n", "-P", &itcp, "-sTCP:LISTEN", "-t"])
            .output()
        {
            if out.status.success() {
                let pids: Vec<String> = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(std::string::ToString::to_string)
                    .collect();
                for pid in &pids {
                    let _ = StdCommand::new("kill").args(["-TERM", pid]).status();
                }
                if !pids.is_empty() {
                    std::thread::sleep(Duration::from_millis(400));
                    return Ok(());
                }
            }
        }
        let _ = StdCommand::new("fuser")
            .args(["-k", &format!("{port}/tcp")])
            .status();
    }
    Ok(())
}

pub async fn start_and_monitor(
    app: &AppHandle,
    port: u16,
    token: &str,
    restart: Arc<Notify>,
    shutdown: Arc<Notify>,
) -> Result<()> {
    let deepseek_bin = find_deepseek_binary(app);

    'supervisor: loop {
        let mut child: Option<tokio::process::Child> = None;

        let ready = sidecar_ready(port, token).await;

        if !ready {
            let (health_ok, api_ok) = tokio::join!(
                is_healthy(port, token),
                runtime_api_accepts_token(port, token)
            );
            if health_ok && !api_ok {
                eprintln!(
                    "deepseek-desktop: {port}/health OK but runtime API rejected this session token; stopping stale listener(s)."
                );
                kill_processes_listening_on_local_port(port).ok();
                sleep(Duration::from_millis(800)).await;
            }

            let c = spawn_sidecar(&deepseek_bin, port, token)?
                .spawn()
                .with_context(|| format!("failed to start sidecar: {deepseek_bin}"))?;
            child = Some(c);

            for i in 0..MAX_STARTUP_RETRIES {
                let delay_ms = if i == 0 {
                    STARTUP_FIRST_DELAY_MS
                } else {
                    STARTUP_RETRY_DELAY_MS
                };
                sleep(Duration::from_millis(delay_ms)).await;
                if sidecar_ready(port, token).await {
                    break;
                }
                if i == MAX_STARTUP_RETRIES - 1 {
                    if let Some(mut ch) = child.take() {
                        ch.kill().await.ok();
                    }
                    let log_tail = sidecar_stderr_log_path()
                        .as_deref()
                        .map(|p| read_log_tail(p, 30))
                        .filter(|s| !s.is_empty() && s != "(log unreadable)")
                        .map(|s| format!("\nsidecar stderr tail:\n{s}"))
                        .unwrap_or_default();
                    anyhow::bail!(
                        "sidecar failed to become healthy after {MAX_STARTUP_RETRIES} retries{log_tail}"
                    );
                }
            }
        }

        let mut failures = 0u32;
        loop {
            tokio::select! {
                biased;
                _ = shutdown.notified() => {
                    eprintln!("deepseek-desktop: shutting down sidecar…");
                    if let Some(mut ch) = child.take() {
                        ch.kill().await.ok();
                    }
                    return Ok(());
                }
                _ = restart.notified() => {
                    eprintln!("deepseek-desktop: restarting sidecar to pick up config changes…");
                    if let Some(mut ch) = child.take() {
                        ch.kill().await.ok();
                    } else {
                        kill_processes_listening_on_local_port(port).ok();
                    }
                    sleep(Duration::from_millis(800)).await;
                    continue 'supervisor;
                }
                _ = sleep(Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS)) => {
                    if sidecar_ready(port, token).await {
                        failures = 0;
                    } else {
                        failures += 1;
                        if failures >= MAX_HEALTH_FAILURES {
                            let log_snippet = sidecar_stderr_log_path()
                                .as_deref()
                                .map(|p| read_log_tail(p, 20))
                                .unwrap_or_default();
                            eprintln!(
                                "deepseek-desktop: sidecar unresponsive ({} health failures); restarting. stderr tail:\n{log_snippet}",
                                failures
                            );
                            if let Some(mut ch) = child.take() {
                                ch.kill().await.ok();
                            } else {
                                kill_processes_listening_on_local_port(port).ok();
                            }
                            sleep(Duration::from_millis(500)).await;
                            continue 'supervisor;
                        }
                    }
                }
            }
        }
    }
}

/// Tauri bundles `externalBin` next to the main executable as `deepseek-tui-<target>(.exe)`.
fn scan_sidecar_dir(dir: &Path) -> Option<PathBuf> {
    let read = std::fs::read_dir(dir).ok()?;
    let mut matches: Vec<PathBuf> = read
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with("deepseek-tui") && n != "deepseek-tui" && n != "deepseek-tui.exe"
            })
        })
        .collect();
    matches.sort();
    matches.into_iter().next()
}

fn bundled_sidecar_path(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Some(p) = scan_sidecar_dir(dir) {
                return Some(p);
            }
        }
    }
    if let Ok(res) = app.path().resource_dir() {
        if let Some(p) = scan_sidecar_dir(&res) {
            return Some(p);
        }
    }
    None
}

fn find_deepseek_binary(app: &AppHandle) -> String {
    if let Some(p) = bundled_sidecar_path(app) {
        return p.to_string_lossy().into_owned();
    }

    let candidates = ["deepseek-tui", "deepseek"];
    for name in &candidates {
        if std::process::Command::new(name)
            .arg("--version")
            .output()
            .is_ok()
        {
            return (*name).to_string();
        }
    }

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    for name in &candidates {
        let path = format!("../../target/{profile}/{name}");
        if Path::new(&path).exists() || Path::new(&format!("{path}.exe")).exists() {
            return path;
        }
    }

    candidates[0].to_string()
}
