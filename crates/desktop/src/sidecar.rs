use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::StatusCode;
use tauri::{AppHandle, Manager};
use tokio::process::Command;
use tokio::sync::Notify;
use tokio::time::sleep;

const HEALTH_CHECK_INTERVAL_SECS: u64 = 5;
const MAX_STARTUP_RETRIES: u32 = 10;
const STARTUP_RETRY_DELAY_MS: u64 = 1000;
const MAX_HEALTH_FAILURES: u32 = 3;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

fn spawn_sidecar(deepseek_bin: &str, port: u16, token: &str) -> Result<Command> {
    let port_s = port.to_string();
    let mut std_cmd = std::process::Command::new(deepseek_bin);
    std_cmd
        .args([
            "serve",
            "--http",
            "--host",
            "127.0.0.1",
            "--port",
            port_s.as_str(),
            "--auth-token",
            token,
            "--cors-origin",
            "http://tauri.localhost",
            "--cors-origin",
            "https://tauri.localhost",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
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
    let client = reqwest::Client::new();
    match client
        .get(&url)
        .timeout(Duration::from_secs(2))
        .send()
        .await
    {
        Ok(resp) => resp.status() == StatusCode::OK,
        Err(_) => false,
    }
}

/// True when `/v1/*` accepts this install's bearer token (not only `/health`).
async fn runtime_api_accepts_token(port: u16, token: &str) -> bool {
    if token.trim().is_empty() {
        return true;
    }
    let url = format!("http://127.0.0.1:{port}/v1/sessions");
    let client = reqwest::Client::new();
    match client
        .get(&url)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .timeout(Duration::from_secs(2))
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
) -> Result<()> {
    let deepseek_bin = find_deepseek_binary(app);

    'supervisor: loop {
        let mut child: Option<tokio::process::Child> = None;

        let ready = is_healthy(port, token).await && runtime_api_accepts_token(port, token).await;

        if !ready {
            if is_healthy(port, token).await && !runtime_api_accepts_token(port, token).await {
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
                sleep(Duration::from_millis(STARTUP_RETRY_DELAY_MS)).await;
                if is_healthy(port, token).await && runtime_api_accepts_token(port, token).await {
                    break;
                }
                if i == MAX_STARTUP_RETRIES - 1 {
                    if let Some(mut ch) = child.take() {
                        ch.kill().await.ok();
                    }
                    anyhow::bail!(
                        "sidecar failed to become healthy after {MAX_STARTUP_RETRIES} retries"
                    );
                }
            }
        }

        let mut failures = 0u32;
        loop {
            tokio::select! {
                biased;
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
                    if is_healthy(port, token).await && runtime_api_accepts_token(port, token).await {
                        failures = 0;
                    } else {
                        failures += 1;
                        if failures >= MAX_HEALTH_FAILURES {
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
