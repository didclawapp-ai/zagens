use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use reqwest::StatusCode;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{Notify, mpsc, oneshot, watch};
use tokio::time::sleep;

const HEALTH_CHECK_INTERVAL_SECS: u64 = 5;
/// After spawning the sidecar, poll quickly so we detect `/health` as soon as it binds (1s was
/// a fixed blind wait that added up to ~1s latency on fast starts and aligned poorly with UI polls).
const MAX_STARTUP_RETRIES: u32 = 60;
const STARTUP_FIRST_DELAY_MS: u64 = 60;
const STARTUP_RETRY_DELAY_MS: u64 = 200;
/// Desktop probes loopback repeatedly; transient stalls (heavy git restore under the thread
/// workspace, AV scanning on Windows, etc.) can exceed a short HTTP timeout without the sidecar
/// being dead — too aggressive restarts surfaced as ERR_CONNECTION_RESET on in-flight `/v1/*`.
/// Differentiated thresholds: connection-refused means the port is dead (immediate action),
/// busy-timeout means the process is alive but overloaded (grace period).
const MAX_CONNECT_REFUSED: u32 = 2;
const MAX_BUSY_TIMEOUTS: u32 = 12;
/// Coalesce rapid `sidecar_restart.notify_one()` bursts (e.g. saving API key + vision settings).
const RESTART_DEBOUNCE_MS: u64 = 450;
/// Poll while deferring a config restart until all turns are idle.
const RESTART_ACTIVE_POLL_MS: u64 = 2000;
/// Poll interval while waiting for `127.0.0.1:port` to drop LISTEN after `kill` (Windows EADDRINUSE / 10048).
const PORT_FREE_POLL_MS: u64 = 75;
const PORT_FREE_MAX_WAIT_MS: u64 = 10_000;
/// Rapid sidecar crash backoff: if the sidecar restarts ≥ N times within WINDOW,
/// pause auto-restart and warn the user instead of looping indefinitely.
const MAX_RAPID_RESTARTS: usize = 3;
const RAPID_RESTART_WINDOW_SECS: u64 = 60;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Shared client for localhost probes (avoids reconstructing TLS stacks per request on Windows).
static SIDECAR_PROBE_HTTP: LazyLock<Option<reqwest::Client>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(400))
        .timeout(Duration::from_secs(3))
        .build()
        .ok()
});

fn sidecar_probe_http() -> Option<&'static reqwest::Client> {
    SIDECAR_PROBE_HTTP.as_ref()
}

/// Avoid inheriting System32/System as the implicit cwd for embedded `deepseek serve`:
/// tooling defaults (and broken session resumes) would otherwise latch onto that directory.
fn sidecar_spawn_cwd() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// Path to the sidecar stderr log file. Created under `~/.zagens/logs/` on
/// first spawn — the parent directory is ensured before opening the file (#H4).
fn sidecar_stderr_log_path() -> Option<PathBuf> {
    let log_dir = zagens_config::user_data_path("logs").ok()?;
    std::fs::create_dir_all(&log_dir).ok()?;
    Some(log_dir.join("sidecar.log"))
}

/// Zagens parent process (Tauri) supervisor events — same folder as `sidecar.log`, so GUI users
/// without a console still get restart / health-check reasons on disk.
fn supervisor_log_path() -> Option<PathBuf> {
    let log_dir = zagens_config::user_data_path("logs").ok()?;
    std::fs::create_dir_all(&log_dir).ok()?;
    Some(log_dir.join("supervisor.log"))
}

static SUPERVISOR_LOG_MUTEX: Mutex<()> = Mutex::new(());

/// Tell the WebView the sidecar is about to be killed (config save, health restart, etc.).
fn emit_sidecar_restarting(app: &AppHandle, reason: &str) {
    let payload = serde_json::json!({ "reason": reason });
    let _ = app.emit("sidecar://restarting", &payload);
}

/// Config restart is queued because one or more threads still have active turns.
fn emit_sidecar_restart_pending(app: &AppHandle, active_count: usize) {
    let payload = serde_json::json!({ "active_count": active_count });
    let _ = app.emit("sidecar://restart-pending", &payload);
}

fn emit_sidecar_restart_pending_cleared(app: &AppHandle) {
    let _ = app.emit("sidecar://restart-pending-cleared", &serde_json::json!({}));
}

#[derive(Debug, Deserialize)]
struct ActiveTurnsProbe {
    count: usize,
}

async fn fetch_active_turn_count(port: u16, token: &str) -> usize {
    if port == 0 {
        return 0;
    }
    let Some(client) = sidecar_probe_http() else {
        return 0;
    };
    let url = format!("http://127.0.0.1:{port}/v1/runtime/active-turns");
    let mut req = client.get(&url);
    if !token.trim().is_empty() {
        req = req.header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", token.trim()),
        );
    }
    match req.send().await {
        Ok(resp) if resp.status().is_success() => resp
            .json::<ActiveTurnsProbe>()
            .await
            .map(|body| body.count)
            .unwrap_or(0),
        _ => 0,
    }
}

async fn debounce_restart_notifications(restart: &Notify) {
    loop {
        tokio::select! {
            _ = sleep(Duration::from_millis(RESTART_DEBOUNCE_MS)) => break,
            _ = restart.notified() => {}
        }
    }
}

/// Wait until no active turns remain, or the user forces an immediate restart.
async fn wait_for_idle_or_force_restart(
    app: &AppHandle,
    port: u16,
    token: &str,
    restart: &Notify,
    force_now: &AtomicBool,
    shutdown: &Notify,
) -> bool {
    let mut pending_emitted = false;
    loop {
        if force_now.swap(false, Ordering::SeqCst) {
            supervisor_log("event=restart reason=config_change force=true");
            if pending_emitted {
                emit_sidecar_restart_pending_cleared(app);
            }
            return true;
        }
        let active = fetch_active_turn_count(port, token).await;
        if active == 0 {
            if pending_emitted {
                emit_sidecar_restart_pending_cleared(app);
            }
            return true;
        }
        if !pending_emitted {
            emit_sidecar_restart_pending(app, active);
            pending_emitted = true;
        }
        supervisor_log(format!("event=restart_deferred active_turns={active}"));
        tokio::select! {
            biased;
            _ = shutdown.notified() => {
                if pending_emitted {
                    emit_sidecar_restart_pending_cleared(app);
                }
                return false;
            }
            _ = restart.notified() => {
                debounce_restart_notifications(restart).await;
            }
            _ = sleep(Duration::from_millis(RESTART_ACTIVE_POLL_MS)) => {}
        }
    }
}

/// Append a timestamped line to `~/.zagens/logs/supervisor.log` and mirror to stderr.
fn supervisor_log(message: impl AsRef<str>) {
    let msg = message.as_ref().trim_end();
    let stamped = format!(
        "{} {}\n",
        chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f"),
        msg
    );
    if let Ok(_guard) = SUPERVISOR_LOG_MUTEX.lock()
        && let Some(path) = supervisor_log_path()
        && let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    {
        let _ = f.write_all(stamped.as_bytes());
        let _ = f.flush();
    }
    eprintln!("{msg}");
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

/// Bundled PBS Python shipped in the installer (`tauri.conf.json` → `python/`).
fn bundled_python_executable(app: &AppHandle) -> Option<PathBuf> {
    #[cfg(windows)]
    let py_name = "python.exe";
    #[cfg(not(windows))]
    let py_name = if cfg!(target_os = "macos") {
        "python3.12"
    } else {
        "python3"
    };

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        candidates.push(res.join("python").join(py_name));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("python").join(py_name));
        #[cfg(target_os = "macos")]
        candidates.push(dir.join("../Resources/python").join(py_name));
    }

    candidates.into_iter().find(|path| path.is_file())
}

fn runtime_sidecar_cli_args(port: &str) -> Vec<String> {
    let mut args = vec![
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.into(),
    ];
    args.push("--cors-origin".into());
    args.push("http://tauri.localhost".into());
    args.push("--cors-origin".into());
    args.push("https://tauri.localhost".into());
    args
}

fn sidecar_spawn_workspace() -> PathBuf {
    crate::workspace_defaults::default_composer_workspace()
        .map(PathBuf::from)
        .unwrap_or_else(|_| sidecar_spawn_cwd().unwrap_or_else(|| PathBuf::from(".")))
}

fn inject_active_custom_provider_key(cmd: &mut std::process::Command) {
    let Ok(store) = zagens_config::ConfigStore::load(None) else {
        return;
    };
    if store.config.provider != zagens_config::ProviderKind::Custom {
        return;
    }
    let Some(id) = store
        .config
        .custom_provider_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let slot = zagens_config::custom_provider_keyring_slot(id);
    let secrets = zagens_secrets::Secrets::auto_detect();
    if let Some(key) = secrets.resolve(&slot) {
        cmd.env("OPENAI_API_KEY", &key);
        cmd.env("ZAGENS_CUSTOM_API_KEY", &key);
    }
}

fn spawn_sidecar(app: &AppHandle, runtime_bin: &str, port: u16, token: &str) -> Result<Command> {
    let port_s = port.to_string();
    let workspace = sidecar_spawn_workspace();
    let workspace_s = workspace.to_string_lossy().into_owned();
    let mut std_cmd = std::process::Command::new(runtime_bin);
    std_cmd.env("DEEPSEEK_RUNTIME_TOKEN", token);
    std_cmd.env("DEEPSEEK_CLIENT_SURFACE", "zagens");
    // Desktop UI modes (YOLO / trust) may opt in per request; gate in runtime via
    // `Config::effective_trust_mode` requires deployment-level permission.
    std_cmd.env("DEEPSEEK_TRUST_MODE", "1");
    if let Some(py) = bundled_python_executable(app) {
        std_cmd.env("DEEPSEEK_BUNDLED_PYTHON", py);
    }

    // Pull provider API keys from OS keyring into env vars. Runtime resolves
    // credentials via env → config (no keyring I/O in the sidecar process).
    zagens_secrets::inject_keyring_envs(&mut std_cmd);
    inject_active_custom_provider_key(&mut std_cmd);
    std_cmd
        .args(runtime_sidecar_cli_args(port_s.as_str()))
        .arg("--workspace")
        .arg(&workspace_s)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    if let Some(log_path) = sidecar_stderr_log_path() {
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap_or_else(|_| std::fs::File::create(&log_path).unwrap());
        match log_file.try_clone() {
            Ok(stderr_dup) => {
                std_cmd.stderr(Stdio::from(stderr_dup));
            }
            Err(_) => {
                std_cmd.stderr(Stdio::null());
            }
        }
    } else {
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

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct InternalProbeResponse {
    status: String,
    pid: u32,
    started_at_ms: u128,
    token_fingerprint: String,
    version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProbeOutcome {
    Ok,
    Refused,
    Timeout,
    TokenMismatch,
    Other(String),
}

fn compute_token_fingerprint(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let hash = hasher.finalize();
    hash[..16].iter().map(|b| format!("{b:02x}")).collect()
}

async fn probe_sidecar(port: u16, expected_fp: &str) -> ProbeOutcome {
    let Some(client) = sidecar_probe_http() else {
        return ProbeOutcome::Other("sidecar probe HTTP client unavailable".to_string());
    };
    let url = format!("http://127.0.0.1:{port}/internal/probe");
    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<InternalProbeResponse>().await {
                Ok(body) if body.token_fingerprint == expected_fp => ProbeOutcome::Ok,
                Ok(_) => ProbeOutcome::TokenMismatch,
                Err(e) => ProbeOutcome::Other(e.to_string()),
            }
        }
        Ok(resp) => ProbeOutcome::Other(format!("HTTP {}", resp.status())),
        Err(e) if e.is_connect() => ProbeOutcome::Refused,
        Err(e) if e.is_timeout() => ProbeOutcome::Timeout,
        Err(e) => ProbeOutcome::Other(e.to_string()),
    }
}

/// Legacy: kept for fallback compatibility with older sidecars that don't serve /internal/probe.
#[allow(dead_code)]
async fn sidecar_ready_legacy(port: u16, token: &str) -> bool {
    let (health_ok, api_ok) = tokio::join!(
        is_healthy_legacy(port),
        runtime_api_accepts_token_legacy(port, token)
    );
    health_ok && api_ok
}

#[allow(dead_code)]
async fn is_healthy_legacy(port: u16) -> bool {
    let Some(client) = sidecar_probe_http() else {
        return false;
    };
    let url = format!("http://127.0.0.1:{port}/health");
    match client.get(&url).send().await {
        Ok(resp) => resp.status() == StatusCode::OK,
        Err(_) => false,
    }
}

#[allow(dead_code)]
async fn runtime_api_accepts_token_legacy(port: u16, token: &str) -> bool {
    if token.trim().is_empty() {
        return true;
    }
    let Some(client) = sidecar_probe_http() else {
        return false;
    };
    let url = format!("http://127.0.0.1:{port}/v1/sessions");
    match client
        .get(&url)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
    {
        Ok(resp) => resp.status() != StatusCode::UNAUTHORIZED,
        Err(_) => false,
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum PongEvent {
    Pong { seq: u64, pid: u32, uptime_ms: u64 },
    Drain { state: String },
}

/// Carries the parsed `DS_PICK_READY` payload to the supervisor.
/// `port` is the **actually bound** port reported by the sidecar (may differ from
/// the requested port when sidecar binds to `0` for ephemeral allocation).
#[derive(Debug, Clone)]
pub(crate) struct ReadySignal {
    pub port: u16,
}

/// Forwards sidecar stdout lines to sidecar.log and watches for protocol lines.
/// Returns (ready_rx, pong_rx) — ready fires once on DS_PICK_READY (carrying the
/// real bound port), pong_rx receives PongEvent for each DS_PICK_PONG / DS_PICK_DRAIN line.
/// When the READY signal is received, also emits `sidecar://ready` to the WebView.
fn spawn_stdout_forwarder(
    stdout: tokio::process::ChildStdout,
    app: AppHandle,
) -> (
    oneshot::Receiver<ReadySignal>,
    mpsc::UnboundedReceiver<PongEvent>,
) {
    let (ready_tx, ready_rx) = oneshot::channel::<ReadySignal>();
    let (pong_tx, pong_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        let mut ready_tx = Some(ready_tx);
        while let Ok(Some(line)) = reader.next_line().await {
            if let Some(log_path) = sidecar_stderr_log_path()
                && let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
            {
                let _ = writeln!(f, "{line}");
            }
            if let Some(rest) = line.strip_prefix("DS_PICK_READY ") {
                if let Some(tx) = ready_tx.take() {
                    let payload: serde_json::Value = rest.parse().unwrap_or_default();
                    let real_port = payload
                        .get("port")
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|p| u16::try_from(p).ok())
                        .unwrap_or(0);
                    supervisor_log(format!("event=ready_signal port={real_port}"));
                    let _ = app.emit("sidecar://ready", &payload);
                    let _ = tx.send(ReadySignal { port: real_port });
                }
            } else if let Some(payload) = line.strip_prefix("DS_PICK_PONG ") {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) {
                    let seq = v.get("seq").and_then(|s| s.as_u64()).unwrap_or(0);
                    let pid = v.get("pid").and_then(|p| p.as_u64()).unwrap_or(0) as u32;
                    let uptime_ms = v.get("uptime_ms").and_then(|u| u.as_u64()).unwrap_or(0);
                    let _ = pong_tx.send(PongEvent::Pong {
                        seq,
                        pid,
                        uptime_ms,
                    });
                }
            } else if let Some(payload) = line.strip_prefix("DS_PICK_DRAIN ") {
                let state = serde_json::from_str::<serde_json::Value>(payload)
                    .ok()
                    .and_then(|v| v.get("state").and_then(|s| s.as_str()).map(String::from))
                    .unwrap_or_else(|| "draining".to_string());
                supervisor_log(format!("event=drain_received state={state}"));
                let _ = pong_tx.send(PongEvent::Drain { state });
                break;
            }
        }
    });
    (ready_rx, pong_rx)
}

/// Best-effort: stop processes listening on loopback `port` so we can bind a new sidecar.
/// Used when an old sidecar is still bound after the desktop app restarted
/// (new random auth token → `/health` OK but `/v1/*` returns 401).
fn kill_processes_listening_on_local_port(port: u16) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = format!(
            r#"$port = {}; Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue | Where-Object {{ $_.LocalAddress -eq '127.0.0.1' -or $_.LocalAddress -eq '::1' }} | ForEach-Object {{ Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }}"#,
            port
        );
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
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
            && out.status.success()
        {
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
        let _ = StdCommand::new("fuser")
            .args(["-k", &format!("{port}/tcp")])
            .status();
    }
    Ok(())
}

/// Ensure nothing is accepting connections on loopback `port` before spawning a new sidecar.
///
/// After `Child::kill`, Windows often keeps the listener socket around briefly; spawning again
/// immediately hits **error 10048** (WSAEADDRINUSE). We reclaim listeners via PowerShell/lsof then
/// probe-bind until [`TcpListener::bind`] succeeds (listener dropped immediately — child binds next).
async fn wait_loopback_listen_port_free(port: u16, label: &'static str) {
    kill_processes_listening_on_local_port(port).ok();

    let mut elapsed = 0u64;
    loop {
        let available = std::net::TcpListener::bind(("127.0.0.1", port)).is_ok();
        if available {
            if elapsed > 0 {
                supervisor_log(format!(
                    "zagens-desktop: loopback:{port} released after {elapsed}ms ({label})"
                ));
            }
            return;
        }
        if elapsed >= PORT_FREE_MAX_WAIT_MS {
            supervisor_log(format!(
                "zagens-desktop: warning — loopback:{port} still busy after {PORT_FREE_MAX_WAIT_MS}ms ({label}); spawn may fail (EADDRINUSE / 10048)"
            ));
            return;
        }
        sleep(Duration::from_millis(PORT_FREE_POLL_MS)).await;
        elapsed += PORT_FREE_POLL_MS;
    }
}

pub async fn start_and_monitor(
    app: &AppHandle,
    initial_port: u16,
    port_tx: watch::Sender<u16>,
    token: &str,
    restart: Arc<Notify>,
    restart_force: Arc<AtomicBool>,
    shutdown: Arc<Notify>,
) -> Result<()> {
    // `port` may be re-published after each spawn cycle when the sidecar reports the
    // actually bound port via `DS_PICK_READY` (matters when initial_port == 0 or the
    // sidecar falls back to an ephemeral allocation). All probe/restart paths below
    // already read the local `port` variable, so updating it propagates everywhere.
    let mut port = initial_port;
    let runtime_bin = find_runtime_binary(app);
    let token_fp = compute_token_fingerprint(token);
    supervisor_log(format!(
        "event=supervisor_start initial_port={initial_port} sidecar={runtime_bin} token_fp={token_fp}"
    ));
    let mut crash_times: VecDeque<Instant> = VecDeque::new();

    'supervisor: loop {
        let mut child: Option<tokio::process::Child> = None;
        let mut sidecar_stdin: Option<tokio::process::ChildStdin> = None;
        let mut pong_rx: Option<mpsc::UnboundedReceiver<PongEvent>> = None;

        let outcome = probe_sidecar(port, &token_fp).await;

        if outcome != ProbeOutcome::Ok {
            // Reset published port → IPC handlers will await the new READY before issuing
            // requests; without this `get_runtime_port` could return the stale port mid-restart.
            let _ = port_tx.send(0);
            match outcome {
                ProbeOutcome::TokenMismatch => {
                    supervisor_log(format!(
                        "event=token_mismatch port={port} action=reclaim_stale_listener"
                    ));
                    wait_loopback_listen_port_free(port, "stale-runtime-token").await;
                }
                ProbeOutcome::Other(_) => {
                    supervisor_log(format!(
                        "event=pre_spawn_probe port={port} outcome={outcome:?} action=spawn_new_sidecar"
                    ));
                }
                _ => {}
            }

            wait_loopback_listen_port_free(port, "before-sidecar-spawn").await;

            let mut c = spawn_sidecar(app, &runtime_bin, port, token)?
                .spawn()
                .with_context(|| format!("failed to start sidecar: {runtime_bin}"))?;

            let stdout = c.stdout.take();
            sidecar_stdin = c.stdin.take();
            let (ready_rx, rx) = stdout
                .map(|s| spawn_stdout_forwarder(s, app.clone()))
                .unzip();
            pong_rx = rx;
            child = Some(c);

            let startup_t0 = Instant::now();

            // Wait up to 60s for the READY signal from stdout.
            let ready_signal = if let Some(rx) = ready_rx {
                tokio::time::timeout(Duration::from_secs(60), rx)
                    .await
                    .ok()
                    .and_then(|r| r.ok())
            } else {
                None
            };

            if let Some(sig) = ready_signal {
                // Adopt the sidecar's actually bound port (may differ from initial_port
                // when binding to 0 / ephemeral). Propagate it to every IPC handler via
                // the watch channel so `runtime_proxy` / `commands::*` use the real URL.
                if sig.port != 0 && sig.port != port {
                    supervisor_log(format!(
                        "event=port_adopted requested={port} actual={} source=DS_PICK_READY",
                        sig.port
                    ));
                    port = sig.port;
                }
                if sig.port != 0 {
                    let _ = port_tx.send(sig.port);
                } else {
                    // Defensive: legacy sidecar may not report port; fall back to requested.
                    let _ = port_tx.send(port);
                }
                supervisor_log(format!(
                    "event=sidecar_ready signal=stdout startup_ms={} port={port}",
                    startup_t0.elapsed().as_millis()
                ));
            } else {
                // Fallback: HTTP probe loop (legacy sidecar that doesn't print DS_PICK_READY,
                // or stdout forwarder wasn't attached). The requested `port` is the only
                // address we can probe in this fallback — `--port 0` won't work here.
                for i in 0..MAX_STARTUP_RETRIES {
                    let delay_ms = if i == 0 {
                        STARTUP_FIRST_DELAY_MS
                    } else {
                        STARTUP_RETRY_DELAY_MS
                    };
                    sleep(Duration::from_millis(delay_ms)).await;
                    let outcome = probe_sidecar(port, &token_fp).await;
                    if outcome == ProbeOutcome::Ok {
                        let _ = port_tx.send(port);
                        supervisor_log(format!(
                            "event=sidecar_ready signal=http startup_ms={} port={port}",
                            startup_t0.elapsed().as_millis()
                        ));
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
                        supervisor_log(format!(
                            "event=startup_failed retries={MAX_STARTUP_RETRIES} port={port}{log_tail}"
                        ));
                        anyhow::bail!(
                            "sidecar failed to become healthy after {MAX_STARTUP_RETRIES} retries{log_tail}"
                        );
                    }
                }
            }
        }

        crash_times.clear();

        let mut connect_refused: u32 = 0;
        let mut busy_timeouts: u32 = 0;
        let mut ping_seq: u64 = 0;
        loop {
            tokio::select! {
                biased;
                _ = shutdown.notified() => {
                    supervisor_log("event=shutdown action=killing_sidecar");
                    emit_sidecar_restarting(app, "shutdown");
                    if let Some(mut ch) = child.take() {
                        ch.kill().await.ok();
                    }
                    return Ok(());
                }
                _ = restart.notified() => {
                    supervisor_log("event=restart reason=config_change");
                    debounce_restart_notifications(&restart).await;
                    if !wait_for_idle_or_force_restart(
                        app,
                        port,
                        token,
                        &restart,
                        &restart_force,
                        &shutdown,
                    )
                    .await
                    {
                        return Ok(());
                    }
                    emit_sidecar_restarting(app, "config_change");
                    if let Some(mut ch) = child.take() {
                        ch.kill().await.ok();
                    } else {
                        kill_processes_listening_on_local_port(port).ok();
                    }
                    wait_loopback_listen_port_free(port, "config-restart").await;
                    continue 'supervisor;
                }
                _ = sleep(Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS)) => {
                    // Primary probe: ping/pong via stdin/stdout line protocol.
                    // This is physically independent of the axum HTTP stack.
                    let ping_ok = if let Some(ref mut stdin) = sidecar_stdin {
                        // Drain any stale pong events before sending a new ping
                        while pong_rx.as_mut().and_then(|rx| rx.try_recv().ok()).is_some() {}
                        let ping = format!("{{\"op\":\"ping\",\"seq\":{ping_seq}}}\n");
                        if stdin.write_all(ping.as_bytes()).await.is_ok() && stdin.flush().await.is_ok() {
                            ping_seq += 1;
                            // Wait up to 1s for the pong reply via stdout → forwarder → pong_rx
                            if let Some(ref mut rx) = pong_rx {
                                matches!(
                                    tokio::time::timeout(Duration::from_secs(1), rx.recv()).await,
                                    Ok(Some(PongEvent::Pong { .. }))
                                )
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if ping_ok {
                        if connect_refused > 0 || busy_timeouts > 0 {
                            supervisor_log(format!(
                                "event=health_restored signal=ping_pong connect_refused={connect_refused} busy_timeouts={busy_timeouts} port={port}"
                            ));
                        }
                        connect_refused = 0;
                        busy_timeouts = 0;
                    } else {
                        // Fallback: HTTP /internal/probe
                        let outcome = probe_sidecar(port, &token_fp).await;
                        match outcome {
                            ProbeOutcome::Ok => {
                                if connect_refused > 0 || busy_timeouts > 0 {
                                    supervisor_log(format!(
                                        "event=health_restored signal=http connect_refused={connect_refused} busy_timeouts={busy_timeouts} port={port}"
                                    ));
                                }
                                connect_refused = 0;
                                busy_timeouts = 0;
                            }
                            ProbeOutcome::Refused => {
                                connect_refused += 1;
                                supervisor_log(format!(
                                    "event=probe_refused count={connect_refused} port={port}"
                                ));
                            }
                            ProbeOutcome::Timeout => {
                                busy_timeouts += 1;
                                supervisor_log(format!(
                                    "event=probe_busy count={busy_timeouts} port={port}"
                                ));
                            }
                            ProbeOutcome::TokenMismatch => {
                                supervisor_log(format!(
                                    "event=token_mismatch port={port} action=reclaim_and_restart"
                                ));
                                emit_sidecar_restarting(app, "token_mismatch");
                                if let Some(mut ch) = child.take() {
                                    ch.kill().await.ok();
                                } else {
                                    kill_processes_listening_on_local_port(port).ok();
                                }
                                wait_loopback_listen_port_free(port, "token-mismatch").await;
                                continue 'supervisor;
                            }
                            ProbeOutcome::Other(ref msg) => {
                                busy_timeouts += 1;
                                supervisor_log(format!(
                                    "event=probe_error count={busy_timeouts} port={port} error={msg}"
                                ));
                            }
                        }
                    }

                    let child_died = if let Some(ref mut c) = child {
                        match c.try_wait() {
                            Ok(Some(status)) => {
                                supervisor_log(format!(
                                    "event=child_exited status={status:?} port={port}"
                                ));
                                true
                            }
                            Ok(None) => false,
                            Err(e) => {
                                supervisor_log(format!(
                                    "event=child_try_wait_error error={e} port={port}"
                                ));
                                false
                            }
                        }
                    } else {
                        false
                    };

                    let should_restart = child_died
                        || connect_refused >= MAX_CONNECT_REFUSED
                        || busy_timeouts >= MAX_BUSY_TIMEOUTS;

                    if should_restart {
                        let reason = if child_died {
                            "child_exited"
                        } else if connect_refused >= MAX_CONNECT_REFUSED {
                            "connect_refused"
                        } else {
                            "busy_timeout"
                        };
                        supervisor_log(format!(
                            "event=restart reason={reason} connect_refused={connect_refused} busy_timeouts={busy_timeouts} port={port}"
                        ));
                        emit_sidecar_restarting(app, reason);
                        if let Some(mut ch) = child.take() {
                            ch.kill().await.ok();
                        } else {
                            kill_processes_listening_on_local_port(port).ok();
                        }
                        wait_loopback_listen_port_free(port, "health-restart").await;

                        let now = Instant::now();
                        let cutoff = now - Duration::from_secs(RAPID_RESTART_WINDOW_SECS);
                        crash_times.push_back(now);
                        while crash_times.front().is_some_and(|&t| t < cutoff) {
                            crash_times.pop_front();
                        }
                        if crash_times.len() > MAX_RAPID_RESTARTS {
                            let wait_secs = 15u64.saturating_mul(
                                (crash_times.len() - MAX_RAPID_RESTARTS) as u64
                            );
                            supervisor_log(format!(
                                "event=rapid_crash_backoff crashes={} window_s={RAPID_RESTART_WINDOW_SECS} wait_s={wait_secs} port={port}",
                                crash_times.len(),
                            ));
                            sleep(Duration::from_secs(wait_secs)).await;
                            let cutoff2 = Instant::now() - Duration::from_secs(RAPID_RESTART_WINDOW_SECS);
                            while crash_times.front().is_some_and(|&t| t < cutoff2) {
                                crash_times.pop_front();
                            }
                        }

                        continue 'supervisor;
                    }
                }
            }
        }
    }
}

/// Tauri bundles `externalBin` next to the main executable as `zagens-runtime-<target>(.exe)`.
fn scan_sidecar_dir(dir: &Path) -> Option<PathBuf> {
    const PREFIX: &str = "zagens-runtime";
    let read = std::fs::read_dir(dir).ok()?;
    let mut matches: Vec<PathBuf> = read
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with(PREFIX) && n != PREFIX && n != format!("{PREFIX}.exe")
            })
        })
        .collect();
    matches.sort_by_key(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(|n| !n.starts_with(PREFIX))
            .unwrap_or(true)
    });
    matches.into_iter().next()
}

fn bundled_sidecar_path(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
        && let Some(p) = scan_sidecar_dir(dir)
    {
        return Some(p);
    }
    if let Ok(res) = app.path().resource_dir()
        && let Some(p) = scan_sidecar_dir(&res)
    {
        return Some(p);
    }
    None
}

fn find_runtime_binary(app: &AppHandle) -> String {
    if let Some(p) = bundled_sidecar_path(app) {
        return p.to_string_lossy().into_owned();
    }

    let candidates = ["zagens-runtime"];
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
