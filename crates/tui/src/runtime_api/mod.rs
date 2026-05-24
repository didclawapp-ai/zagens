//! Runtime HTTP/SSE API for local DeepSeek automation.

use std::collections::{HashMap, HashSet};

use std::fs;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use anyhow::{Context, Result, anyhow, bail};

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware::{self};

use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};

use crate::automation_manager::{
    AutomationManager, AutomationRecord, AutomationRunRecord, AutomationSchedulerConfig,
    CreateAutomationRequest, SharedAutomationManager, UpdateAutomationRequest, spawn_scheduler,
};
use crate::config::{Config, DEFAULT_TEXT_MODEL};
use crate::mcp::{McpConfig, McpPool, McpServerConfig};
use crate::models::SystemPrompt;
use crate::runtime_threads::{
    CompactThreadRequest, CreateThreadRequest, RuntimeThreadManager, RuntimeThreadManagerConfig,
    SharedRuntimeThreadManager, StartTurnRequest, SteerTurnRequest, ThreadDetail, ThreadListFilter,
    ThreadRecord, TurnItemKind, TurnRecord, UpdateThreadRequest, UsageGroupBy,
};
use crate::session_manager::{
    SavedSession, SessionManager, SessionMetadata, create_saved_session_with_mode,
    default_sessions_dir, update_session,
};
use crate::task_manager::{
    NewTaskRequest, SharedTaskManager, TaskManager, TaskManagerConfig, TaskRecord, TaskSummary,
};

mod auth;
mod automations;
mod blackboards;
mod health;
mod mcp;
mod router;
mod sessions;
mod skills;
mod stream;
mod tasks;
mod threads;
mod usage;
mod workspace;

pub(crate) use blackboards::{get_blackboard, list_blackboards};
pub(crate) use automations::{
    create_automation, delete_automation, get_automation, list_automation_runs, list_automations,
    pause_automation, resume_automation, run_automation, update_automation,
};
pub(crate) use health::{health, internal_probe};
pub(crate) use usage::{get_routing_rules, get_usage, rebuild_symbol_index, set_routing_rules};
pub(crate) use workspace::workspace_status;
pub(crate) use mcp::{
    add_mcp_server, delete_mcp_server, get_mcp_server, list_mcp_servers, list_mcp_tools,
    merge_mcp_config_json, update_mcp_server,
};
pub(crate) use sessions::{
    delete_session, get_resume_task, get_session, list_sessions, resume_session_thread,
    ResumeTaskTracker,
};
pub(crate) use skills::{create_skill, import_skill_local, install_skill_remote, list_skills};
pub(crate) use tasks::{cancel_task, clear_tasks, create_task, get_task, list_tasks};
pub(crate) use threads::{
    browse_thread_workspace, browse_workspace_by_root, compact_thread, create_thread,
    fork_thread, get_thread, get_thread_checklist, get_thread_context, get_thread_scratchpad_status,
    interrupt_thread_turn, list_thread_snapshots, list_threads, list_threads_summary,
    persist_thread_session, read_thread_workspace_file, read_workspace_file_by_root,
    resolve_approval, restore_thread_snapshot, resume_thread, start_thread_turn, steer_thread_turn,
    update_thread,
};

pub use router::build_router;

#[derive(Clone)]
pub struct RuntimeApiState {
    config: Config,
    workspace: PathBuf,
    task_manager: SharedTaskManager,
    runtime_threads: SharedRuntimeThreadManager,
    cors_origins: Vec<String>,
    mcp_config_path: PathBuf,
    automations: SharedAutomationManager,
    runtime_token: Option<String>,
    process_started_at_ms: u128,
    token_fingerprint: Arc<String>,
    shared_session_manager: Arc<SessionManager>,
    resume_tracker: sessions::ResumeTaskTracker,
}

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

#[derive(Debug, Deserialize)]
struct StreamTurnRequest {
    prompt: String,
    model: Option<String>,
    mode: Option<String>,
    workspace: Option<PathBuf>,
    allow_shell: Option<bool>,
    trust_mode: Option<bool>,
    auto_approve: Option<bool>,
    #[serde(default)]
    route_intent: Option<String>,
    #[serde(default)]
    task_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResolveApprovalRequest {
    tool_call_id: String,
    decision: String,
}

#[derive(Debug, Clone, Serialize)]
struct WorkspaceStatusResponse {
    workspace: PathBuf,
    git_repo: bool,
    branch: Option<String>,
    staged: usize,
    unstaged: usize,
    untracked: usize,
    ahead: Option<u32>,
    behind: Option<u32>,
}

/// Accept `true`/`false`, `1`/`0`, and `yes`/`no` in query strings (desktop used `replay_only=1`).
fn deserialize_query_bool_option<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Option<String> = Option::deserialize(deserializer)?;
    match raw.as_deref() {
        None => Ok(None),
        Some("") => Ok(None),
        Some("1") | Some("true") | Some("True") | Some("yes") | Some("Yes") => Ok(Some(true)),
        Some("0") | Some("false") | Some("False") | Some("no") | Some("No") => Ok(Some(false)),
        Some(other) => Err(serde::de::Error::custom(format!(
            "invalid boolean for replay_only: '{other}' (use true/false or 1/0)"
        ))),
    }
}

#[derive(Debug, Deserialize)]
struct ThreadEventsQuery {
    since_seq: Option<u64>,
    /// When true, emit persisted backlog only and close — for desktop session restore replay.
    #[serde(default, deserialize_with = "deserialize_query_bool_option")]
    replay_only: Option<bool>,
}

/// Start the runtime API server.
pub async fn run_http_server(
    config: Config,
    workspace: PathBuf,
    options: RuntimeApiOptions,
) -> Result<()> {
    if options.port == 0 {
        bail!("Port must be > 0");
    }

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
    let port = options.port;
    let state = RuntimeApiState {
        config: config.clone(),
        workspace,
        task_manager,
        runtime_threads,
        cors_origins: options.cors_origins.clone(),
        mcp_config_path: config.mcp_config_path(),
        automations,
        runtime_token,
        process_started_at_ms,
        token_fingerprint,
        shared_session_manager,
        resume_tracker: sessions::ResumeTaskTracker::new(),
    };
    let app = build_router(state);

    let addr: SocketAddr = format!("{}:{}", options.host, options.port)
        .parse()
        .with_context(|| format!("Invalid bind address '{}:{}'", options.host, options.port))?;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind {addr}"))?;

    eprintln!(
        "[deepseek-runtime] bound {addr}, serving (+{:?}) — output also on stderr (see sidecar.log if launched from DS Pick)",
        t0.elapsed()
    );
    eprintln!("Runtime API listening on http://{addr}");
    eprintln!("Security: this server is local-first. Do not expose it to untrusted networks.");
    if auth_enabled {
        eprintln!("Runtime API auth: bearer token required for /v1/* routes.");
    }

    // Signal READY to the supervisor via stdout (line protocol).
    // DS Pick's supervisor waits for this line before considering the sidecar healthy.
    let ready_line = serde_json::json!({
        "port": port,
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
        "[deepseek-runtime] axum::serve started, listening on {addr}"
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

pub(crate) fn truncate_text(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{truncated}...")
}

const DEFAULT_CORS_ORIGINS: &[&str] = &[
    "http://localhost:3000",
    "http://127.0.0.1:3000",
    "http://localhost:1420",
    "http://127.0.0.1:1420",
    "tauri://localhost",
    // Tauri 2 WebView2 uses this origin for `fetch` to loopback (desktop shell).
    "http://tauri.localhost",
    "https://tauri.localhost",
];

pub(crate) fn cors_layer(extra_origins: &[String]) -> CorsLayer {
    let mut origins: Vec<HeaderValue> = DEFAULT_CORS_ORIGINS
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    for raw in extra_origins {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        match HeaderValue::from_str(trimmed) {
            Ok(value) if !origins.contains(&value) => origins.push(value),
            Ok(_) => {}
            Err(err) => tracing::warn!(
                "Ignoring invalid CORS origin '{trimmed}': {err}; expected scheme://host[:port]"
            ),
        }
    }
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
}

pub(crate) fn map_thread_err(err: anyhow::Error) -> ApiError {
    let message = err.to_string();
    if message.contains("not found") {
        ApiError::not_found(message)
    } else if message.contains("already has an active turn")
        || message.contains("No active turn")
        || message.contains("is not active")
        || message.contains("no pending approval for")
        || message.contains("pending approval scope mismatch")
    {
        ApiError {
            status: StatusCode::CONFLICT,
            message,
        }
    } else {
        ApiError::bad_request(message)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError::internal(e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        use deepseek_core::error_taxonomy::ErrorEnvelope;

        let status_recoverable = matches!(
            self.status,
            StatusCode::INTERNAL_SERVER_ERROR
                | StatusCode::BAD_GATEWAY
                | StatusCode::SERVICE_UNAVAILABLE
                | StatusCode::GATEWAY_TIMEOUT
                | StatusCode::REQUEST_TIMEOUT
                | StatusCode::TOO_MANY_REQUESTS
        );
        let mut envelope = ErrorEnvelope::classify(&self.message, status_recoverable);
        envelope.recoverable = envelope.recoverable || status_recoverable;
        let body = envelope.to_wire_error_body(self.status.as_u16());
        (self.status, Json(body)).into_response()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
