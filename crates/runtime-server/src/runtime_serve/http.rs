//! HTTP sidecar bootstrap — DI assembly and axum serve loop (D16 E1-d).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use sha2::{Digest, Sha256};
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::automation_manager::{
    AutomationManager, AutomationSchedulerConfig, spawn_scheduler,
};
use crate::config::Config;
use crate::runtime_api::{build_router, ResumeTaskTracker, RuntimeApiState};
use crate::runtime_threads::{RuntimeThreadManager, RuntimeThreadManagerConfig};
use crate::session_manager::{default_sessions_dir, SessionManager};
use crate::task_manager::{TaskManager, TaskManagerConfig};

#[derive(Debug, Clone)]
pub struct RuntimeApiOptions {
    pub host: String,
    pub port: u16,
    pub workers: usize,
    /// Additional CORS origins to allow on top of the built-in defaults
    /// (`http://localhost:{3000,1420}`, `http://127.0.0.1:{3000,1420}`,
    /// `tauri://localhost`). Populated by `--cors-origin` (repeatable),
    /// `DEEPSEEK_CORS_ORIGINS` (comma-separated), and `[runtime_api]
    /// cors_origins` in `config.toml`. Whalescale#255 / #561.
    pub cors_origins: Vec<String>,
    /// Optional bearer token required for `/v1/*` routes. If omitted here,
    /// `run_http_server` also checks `DEEPSEEK_RUNTIME_TOKEN`.
    pub auth_token: Option<String>,
}

impl Default for RuntimeApiOptions {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 7878,
            workers: 8,
            cors_origins: Vec::new(),
            auth_token: None,
        }
    }
}

/// Start the runtime API server.
///
/// `options.port == 0` is now accepted and means "let the OS pick an ephemeral
/// port". The actually bound port is reported back to the supervisor via the
/// `DS_PICK_READY` line (`port: <bound>`) and through the `local_addr().port()`
/// log line below; Zagens desktop consumes it via `tokio::sync::watch::<u16>`
/// (see `crates/desktop/src/sidecar.rs` D2 work). The guard that previously
/// rejected port 0 was removed in this commit (D2 follow-up).
pub async fn run_http_server(
    config: Config,
    workspace: PathBuf,
    options: RuntimeApiOptions,
) -> Result<()> {
    let t0 = std::time::Instant::now();
    eprintln!("[deepseek-runtime] starting HTTP API (task manager, threads, scheduler)…");

    let task_cfg = TaskManagerConfig::from_runtime(
        &config,
        workspace.clone(),
        config.default_text_model.clone(),
        Some(options.workers),
    );
    let manager_cfg = RuntimeThreadManagerConfig::from_task_data_dir(task_cfg.data_dir.clone());
    let sb_config = config.clone();
    let sb_workspace = workspace.clone();
    let runtime_threads = Arc::new(
        tokio::task::spawn_blocking(move || {
            RuntimeThreadManager::open(sb_config, sb_workspace, manager_cfg)
        })
        .await
        .map_err(|e| anyhow!("RuntimeThreadManager::open panicked: {e}"))??,
    );
    eprintln!(
        "[deepseek-runtime] RuntimeThreadManager::open ok (+{:?})",
        t0.elapsed()
    );
    let task_manager =
        TaskManager::start_with_runtime_manager(task_cfg, config.clone(), runtime_threads.clone())
            .await?;
    eprintln!(
        "[deepseek-runtime] TaskManager::start ok (+{:?})",
        t0.elapsed()
    );
    let automations = Arc::new(Mutex::new(AutomationManager::default_location()?));
    runtime_threads.attach_automation_manager(automations.clone());
    let scheduler_cancel = CancellationToken::new();
    let scheduler_handle = spawn_scheduler(
        automations.clone(),
        task_manager.clone(),
        scheduler_cancel.clone(),
        AutomationSchedulerConfig::default(),
    );

    let sessions_dir = default_sessions_dir().unwrap_or_else(|_| {
        dirs::home_dir()
            .map(|h| h.join(".deepseek").join("sessions"))
            .unwrap_or_else(|| PathBuf::from(".deepseek").join("sessions"))
    });
    let runtime_token = options
        .auth_token
        .clone()
        .or_else(|| std::env::var("DEEPSEEK_RUNTIME_TOKEN").ok())
        .filter(|token| !token.trim().is_empty());
    let auth_enabled = runtime_token.is_some();

    let process_started_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let token_fingerprint = {
        let mut hasher = Sha256::new();
        hasher.update(runtime_token.as_deref().unwrap_or(""));
        let hash = hasher.finalize();
        let fp: String = hash[..16].iter().map(|b| format!("{b:02x}")).collect();
        Arc::new(fp)
    };
    let shared_session_manager = Arc::new(
        SessionManager::new(sessions_dir.clone())
            .context("Failed to create SessionManager")?,
    );

    let token_fp = token_fingerprint.as_ref().clone();
    let state = RuntimeApiState::new(
        config.clone(),
        workspace,
        task_manager,
        runtime_threads,
        options.cors_origins.clone(),
        config.mcp_config_path(),
        automations,
        runtime_token,
        process_started_at_ms,
        token_fingerprint,
        shared_session_manager,
        ResumeTaskTracker::new(),
    );
    let app = build_router(state);

    let addr: SocketAddr = format!("{}:{}", options.host, options.port)
        .parse()
        .with_context(|| format!("Invalid bind address '{}:{}'", options.host, options.port))?;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind {addr}"))?;
    // Report the actual bound port so callers that pass `--port 0` (or hit ephemeral fallback)
    // can discover the real listener; `options.port` may differ from `local_addr().port()`.
    let bound_addr = listener
        .local_addr()
        .with_context(|| "Failed to read bound local_addr from TcpListener")?;
    let bound_port = bound_addr.port();

    eprintln!(
        "[deepseek-runtime] bound {bound_addr}, serving (+{:?}) — output also on stderr (see sidecar.log if launched from Zagens)",
        t0.elapsed()
    );
    eprintln!("Runtime API listening on http://{bound_addr}");
    eprintln!("Security: this server is local-first. Do not expose it to untrusted networks.");
    if auth_enabled {
        eprintln!("Runtime API auth: bearer token required for /v1/* routes.");
    }

    // Signal READY to the supervisor via stdout (line protocol).
    // Zagens's supervisor waits for this line before considering the sidecar healthy.
    // `port` MUST be the actually bound port (not the requested one) so the desktop
    // shell can discover ephemeral ports from `--port 0`.
    let ready_line = serde_json::json!({
        "port": bound_port,
        "pid": std::process::id(),
        "token_fp": token_fp,
        "version": env!("CARGO_PKG_VERSION"),
    });
    println!("DS_PICK_READY {ready_line}");
    let _ = std::io::Write::flush(&mut std::io::stdout());

    let started_at = std::time::Instant::now();
    tokio::spawn(async move {
        let stdin = BufReader::new(tokio::io::stdin());
        let mut lines = stdin.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let op: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            match op.get("op").and_then(|v| v.as_str()) {
                Some("ping") => {
                    let seq = op.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);
                    let pong = serde_json::json!({
                        "op": "pong",
                        "seq": seq,
                        "pid": std::process::id(),
                        "uptime_ms": started_at.elapsed().as_millis(),
                    });
                    println!("DS_PICK_PONG {pong}");
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
                Some("drain") => {
                    let drain_resp = serde_json::json!({
                        "op": "drain",
                        "state": "draining",
                    });
                    println!("DS_PICK_DRAIN {drain_resp}");
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                    break;
                }
                _ => {}
            }
        }
    });

    eprintln!(
        "[deepseek-runtime] axum::serve started, listening on {bound_addr}"
    );
    let serve_result = axum::serve(listener, app)
        .await
        .map_err(|e| anyhow!("Runtime API server error: {e}"));
    eprintln!(
        "[deepseek-runtime] axum::serve returned: {:?}",
        serve_result.as_ref().map(|_| "ok").map_err(|e| format!("{e:#}"))
    );
    scheduler_cancel.cancel();
    scheduler_handle.abort();
    serve_result
}
