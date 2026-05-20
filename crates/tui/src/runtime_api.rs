//! Runtime HTTP/SSE API for local DeepSeek automation.

use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::fs;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use anyhow::{Context, Result, anyhow, bail};
use async_stream::stream;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
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
use crate::skills::SkillRegistry;
use crate::skills::install::{
    self, DEFAULT_MAX_SIZE_BYTES, InstallError, InstallOutcome, InstallSource,
    import_local_directory,
};
use crate::snapshot::SnapshotRepo;
use crate::task_manager::{
    NewTaskRequest, SharedTaskManager, TaskManager, TaskManagerConfig, TaskRecord, TaskSummary,
};

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
    resume_tracker: ResumeTaskTracker,
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
struct ResolveApprovalRequest {
    tool_call_id: String,
    decision: String,
}

#[derive(Debug, Clone, Serialize)]
struct ResumeTaskState {
    thread_id: String,
    session_id: String,
    state: String,
    items_written: usize,
    items_total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Clone)]
struct ResumeTaskTracker {
    tasks: Arc<Mutex<HashMap<String, ResumeTaskState>>>,
    seed_gate: Arc<Semaphore>,
}

impl ResumeTaskTracker {
    fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            seed_gate: Arc::new(Semaphore::new(1)),
        }
    }

    async fn register(&self, thread_id: &str, session_id: &str, items_total: usize) {
        let mut tasks = self.tasks.lock().await;
        tasks.insert(
            thread_id.to_string(),
            ResumeTaskState {
                thread_id: thread_id.to_string(),
                session_id: session_id.to_string(),
                state: "seeding".to_string(),
                items_written: 0,
                items_total,
                error: None,
            },
        );
    }

    async fn mark_ready(&self, thread_id: &str) {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.get_mut(thread_id) {
            task.state = "ready".to_string();
            task.items_written = task.items_total;
        }
    }

    async fn mark_error(&self, thread_id: &str, error: String) {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.get_mut(thread_id) {
            task.state = "error".to_string();
            task.error = Some(error);
        }
    }

    async fn get(&self, thread_id: &str) -> Option<ResumeTaskState> {
        let tasks = self.tasks.lock().await;
        tasks.get(thread_id).cloned()
    }

    async fn acquire_seed_permit(&self) -> tokio::sync::OwnedSemaphorePermit {
        self.seed_gate
            .clone()
            .acquire_owned()
            .await
            .expect("seed gate semaphore closed")
    }
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    mode: &'static str,
}

#[derive(Debug, Serialize)]
struct InternalProbeResponse {
    status: &'static str,
    pid: u32,
    started_at_ms: u128,
    token_fingerprint: String,
    version: &'static str,
}

#[derive(Debug, Serialize)]
struct SessionsResponse {
    sessions: Vec<SessionMetadata>,
}

#[derive(Debug, Serialize)]
struct SessionDetailResponse {
    metadata: SessionMetadata,
    messages: Vec<serde_json::Value>,
    system_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResumeSessionRequest {
    model: Option<String>,
    mode: Option<String>,
    #[serde(default)]
    task_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct ResumeSessionResponse {
    thread_id: String,
    session_id: String,
    message_count: usize,
    state: String,
}

#[derive(Debug, Serialize)]
struct TasksResponse {
    tasks: Vec<TaskSummary>,
    counts: crate::task_manager::TaskCounts,
}

#[derive(Debug, Deserialize)]
struct SessionsQuery {
    limit: Option<usize>,
    search: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TasksQuery {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ThreadsQuery {
    limit: Option<usize>,
    include_archived: Option<bool>,
    /// When `true`, returns archived threads only (overrides `include_archived`).
    /// Whalescale#260 / #563.
    archived_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ThreadSummaryQuery {
    limit: Option<usize>,
    search: Option<String>,
    include_archived: Option<bool>,
    /// When `true`, returns archived threads only (overrides `include_archived`).
    /// Whalescale#260 / #563.
    archived_only: Option<bool>,
}

fn resolve_thread_filter(
    include_archived: Option<bool>,
    archived_only: Option<bool>,
) -> ThreadListFilter {
    if archived_only.unwrap_or(false) {
        ThreadListFilter::ArchivedOnly
    } else if include_archived.unwrap_or(false) {
        ThreadListFilter::IncludeArchived
    } else {
        ThreadListFilter::ActiveOnly
    }
}

#[derive(Debug, Serialize)]
struct ThreadSummary {
    id: String,
    title: String,
    preview: String,
    model: String,
    mode: String,
    archived: bool,
    updated_at: chrono::DateTime<Utc>,
    latest_turn_id: Option<String>,
    latest_turn_status: Option<String>,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
struct SkillEntry {
    name: String,
    description: String,
    path: PathBuf,
}

#[derive(Debug, Serialize)]
struct SkillsResponse {
    directory: PathBuf,
    warnings: Vec<String>,
    skills: Vec<SkillEntry>,
}

#[derive(Debug, Deserialize)]
struct CreateSkillRequest {
    /// Directory name for the skill (becomes `<skills_root>/<name>/SKILL.md`).
    name: String,
    /// `global` → configured skills dir (default `~/.deepseek/skills`). `workspace`
    /// → workspace `.agents/skills` or `skills/`, creating `.agents/skills` when
    /// neither exists. Ignored when `parent_directory` is set.
    #[serde(default = "default_create_skill_scope")]
    scope: String,
    /// Optional skills root chosen in the desktop folder picker; must match an
    /// allowed root after canonicalization (global dir or an existing workspace
    /// skills directory).
    #[serde(default)]
    parent_directory: Option<PathBuf>,
}

fn default_create_skill_scope() -> String {
    "workspace".to_string()
}

#[derive(Debug, Serialize)]
struct CreateSkillResponse {
    skill: SkillEntry,
    /// Directory scanned by `GET /v1/skills` (may differ from `skills_root`).
    directory: PathBuf,
    /// Skills root where this skill was created (`<skills_root>/<name>/SKILL.md`).
    skills_root: PathBuf,
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ImportSkillLocalRequest {
    /// Directory containing `SKILL.md` (the skill bundle root).
    source_directory: PathBuf,
    #[serde(default = "default_create_skill_scope")]
    scope: String,
    #[serde(default)]
    parent_directory: Option<PathBuf>,
    /// When true, replace an existing skill directory with the same frontmatter name.
    #[serde(default)]
    replace: bool,
}

#[derive(Debug, Deserialize)]
struct InstallSkillRemoteRequest {
    /// `github:owner/repo`, registry name, or direct tarball URL.
    spec: String,
    #[serde(default = "default_create_skill_scope")]
    scope: String,
    #[serde(default)]
    parent_directory: Option<PathBuf>,
    #[serde(default)]
    replace: bool,
}

#[derive(Debug, Serialize)]
struct McpServerEntry {
    name: String,
    enabled: bool,
    required: bool,
    command: Option<String>,
    url: Option<String>,
    args: Vec<String>,
    connected: bool,
    enabled_tools: Vec<String>,
    disabled_tools: Vec<String>,
}

#[derive(Debug, Serialize)]
struct McpServersResponse {
    servers: Vec<McpServerEntry>,
}

#[derive(Debug, Deserialize)]
struct McpToolsQuery {
    server: Option<String>,
}

#[derive(Debug, Serialize)]
struct McpToolEntry {
    server: String,
    name: String,
    prefixed_name: String,
    description: Option<String>,
    input_schema: Value,
}

#[derive(Debug, Serialize)]
struct McpToolsResponse {
    tools: Vec<McpToolEntry>,
}

#[derive(Debug, Deserialize)]
struct McpAddServerRequest {
    name: String,
    command: Option<String>,
    url: Option<String>,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AutomationRunsQuery {
    limit: Option<usize>,
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

#[derive(Debug, Serialize)]
struct StartTurnResponse {
    thread: ThreadRecord,
    turn: TurnRecord,
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
        resume_tracker: ResumeTaskTracker::new(),
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

pub fn build_router(state: RuntimeApiState) -> Router {
    let api_routes = Router::new()
        .route("/v1/sessions", get(list_sessions))
        .route("/v1/sessions/{id}", get(get_session).delete(delete_session))
        .route(
            "/v1/sessions/{id}/resume-thread",
            post(resume_session_thread),
        )
        .route("/v1/resume-tasks/{thread_id}", get(get_resume_task))
        .route("/v1/workspace/status", get(workspace_status))
        .route("/v1/workspace/browse", get(browse_workspace_by_root))
        .route("/v1/workspace/file", get(read_workspace_file_by_root))
        .route("/v1/stream", post(stream_turn))
        .route("/v1/threads", get(list_threads).post(create_thread))
        .route("/v1/threads/summary", get(list_threads_summary))
        .route("/v1/threads/{id}", get(get_thread).patch(update_thread))
        .route("/v1/threads/{id}/checklist", get(get_thread_checklist))
        .route(
            "/v1/threads/{id}/scratchpad/status",
            get(get_thread_scratchpad_status),
        )
        .route("/v1/threads/{id}/context", get(get_thread_context))
        .route("/v1/threads/{id}/resume", post(resume_thread))
        .route("/v1/threads/{id}/fork", post(fork_thread))
        .route("/v1/threads/{id}/turns", post(start_thread_turn))
        .route(
            "/v1/threads/{id}/turns/{turn_id}/steer",
            post(steer_thread_turn),
        )
        .route(
            "/v1/threads/{id}/turns/{turn_id}/resolve-approval",
            post(resolve_approval),
        )
        .route(
            "/v1/threads/{id}/turns/{turn_id}/interrupt",
            post(interrupt_thread_turn),
        )
        .route("/v1/threads/{id}/compact", post(compact_thread))
        .route(
            "/v1/threads/{id}/persist-session",
            post(persist_thread_session),
        )
        .route("/v1/threads/{id}/snapshots", get(list_thread_snapshots))
        .route(
            "/v1/threads/{id}/snapshots/restore",
            post(restore_thread_snapshot),
        )
        .route(
            "/v1/threads/{id}/workspace/browse",
            get(browse_thread_workspace),
        )
        .route(
            "/v1/threads/{id}/workspace/file",
            get(read_thread_workspace_file),
        )
        .route("/v1/threads/{id}/events", get(stream_thread_events))
        .route("/v1/tasks", get(list_tasks).post(create_task))
        .route("/v1/tasks/{id}", get(get_task))
        .route("/v1/tasks/{id}/cancel", post(cancel_task))
        .route("/v1/skills", get(list_skills).post(create_skill))
        .route("/v1/skills/import", post(import_skill_local))
        .route("/v1/skills/install", post(install_skill_remote))
        .route(
            "/v1/apps/mcp/servers",
            get(list_mcp_servers).post(add_mcp_server),
        )
        .route(
            "/v1/apps/mcp/servers/{name}",
            get(get_mcp_server)
                .put(update_mcp_server)
                .delete(delete_mcp_server),
        )
        .route("/v1/apps/mcp/config/merge", post(merge_mcp_config_json))
        .route("/v1/apps/mcp/tools", get(list_mcp_tools))
        .route(
            "/v1/automations",
            get(list_automations).post(create_automation),
        )
        .route(
            "/v1/automations/{id}",
            get(get_automation)
                .patch(update_automation)
                .delete(delete_automation),
        )
        .route("/v1/automations/{id}/run", post(run_automation))
        .route("/v1/automations/{id}/pause", post(pause_automation))
        .route("/v1/automations/{id}/resume", post(resume_automation))
        .route("/v1/automations/{id}/runs", get(list_automation_runs))
        .route("/v1/usage", get(get_usage))
        .route(
            "/v1/apps/routing/rules",
            get(get_routing_rules).put(set_routing_rules),
        )
        .route(
            "/v1/symbol-index/rebuild",
            post(rebuild_symbol_index),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_runtime_token,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/internal/probe", get(internal_probe))
        .merge(api_routes)
        .layer(cors_layer(&state.cors_origins))
        .with_state(state)
}

async fn require_runtime_token(
    State(state): State<RuntimeApiState>,
    req: Request,
    next: Next,
) -> Response {
    let Some(expected) = state.runtime_token.as_deref() else {
        return next.run(req).await;
    };
    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected)
        || req
            .headers()
            .get("x-deepseek-runtime-token")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|token| token == expected);

    if authorized {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": {
                    "message": "runtime API bearer token required",
                    "status": StatusCode::UNAUTHORIZED.as_u16(),
                }
            })),
        )
            .into_response()
    }
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "deepseek-runtime-api",
        mode: "local",
    })
}

async fn internal_probe(State(state): State<RuntimeApiState>) -> Json<InternalProbeResponse> {
    Json(InternalProbeResponse {
        status: "ok",
        pid: std::process::id(),
        started_at_ms: state.process_started_at_ms,
        token_fingerprint: state.token_fingerprint.as_ref().clone(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn list_sessions(
    State(state): State<RuntimeApiState>,
    Query(query): Query<SessionsQuery>,
) -> Result<Json<SessionsResponse>, ApiError> {
    let manager = state.shared_session_manager.clone();
    let search = query.search.clone();
    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let mut sessions = tokio::task::spawn_blocking(move || {
        if let Some(search) = search {
            manager
                .search_sessions(&search)
                .map_err(|e| format!("Failed to search sessions: {e}"))
        } else {
            manager
                .list_sessions()
                .map_err(|e| format!("Failed to list sessions: {e}"))
        }
    })
    .await
    .map_err(|e| ApiError::internal(format!("session list task panicked: {e}")))?
    .map_err(|e: String| ApiError::internal(e))?;
    sessions.truncate(limit);
    Ok(Json(SessionsResponse { sessions }))
}

async fn get_session(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<SessionDetailResponse>, ApiError> {
    let manager = state.shared_session_manager.clone();
    let detail = tokio::task::spawn_blocking({
        let id = id.clone();
        move || -> std::io::Result<SessionDetailResponse> {
            let session = manager.load_session(&id)?;
            Ok(session_to_detail(session))
        }
    })
    .await
    .map_err(|e| ApiError::internal(format!("get session task panicked: {e}")))?
    .map_err(|e| map_session_err(&id, e, "read"))?;
    Ok(Json(detail))
}

async fn resume_session_thread(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<ResumeSessionRequest>,
) -> Result<(StatusCode, Json<ResumeSessionResponse>), ApiError> {
    let t0 = std::time::Instant::now();
    let manager = state.shared_session_manager.clone();
    let session = tokio::task::spawn_blocking({
        let id = id.clone();
        move || -> Result<SavedSession, std::io::Error> {
            manager.load_session(&id)
        }
    })
    .await
    .map_err(|e| ApiError::internal(format!("resume session task panicked: {e}")))?
    .map_err(|e| map_session_err(&id, e, "read"))?;
    let t1 = t0.elapsed();
    eprintln!(
        "[resume-session] load_session done in {:.1}s, {} messages",
        t1.as_secs_f64(),
        session.messages.len(),
    );

    let model = req.model.unwrap_or_else(|| session.metadata.model.clone());
    let mode = req.mode.unwrap_or_else(|| {
        session
            .metadata
            .mode
            .clone()
            .unwrap_or_else(|| "agent".to_string())
    });

    let workspace = session.metadata.workspace.clone();
    let task_type = crate::task_type::resolve_task_type(
        req.task_type.as_deref(),
        &workspace,
        None,
    );

    // Reuse the persisted runtime thread when it still has events so DS Pick can
    // replay tool cards and thinking after app restart (instead of seeding a blank thread).
    if let Some(ref stored_tid) = session.metadata.runtime_thread_id {
        let stored_tid = stored_tid.trim();
        if !stored_tid.is_empty() {
            if state.runtime_threads.load_thread_sync(stored_tid).is_ok() {
                let has_events = state
                    .runtime_threads
                    .events_since(stored_tid, Some(0))
                    .map(|events| !events.is_empty())
                    .unwrap_or(false);
                if has_events {
                    eprintln!(
                        "[resume-session] reusing runtime thread {stored_tid} (events present)"
                    );
                    return Ok((
                        StatusCode::OK,
                        Json(ResumeSessionResponse {
                            thread_id: stored_tid.to_string(),
                            session_id: id,
                            message_count: session.messages.len(),
                            state: "ready".to_string(),
                        }),
                    ));
                }
            }
        }
    }

    let thread = state
        .runtime_threads
        .create_thread(CreateThreadRequest {
            model: Some(model),
            workspace: Some(workspace),
            mode: Some(mode),
            allow_shell: None,
            trust_mode: None,
            auto_approve: None,
            archived: false,
            system_prompt: session.system_prompt.clone(),
            task_id: None,
            task_type: Some(task_type.as_str().to_string()),
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create thread: {e}")))?;

    let msgs = session.messages;
    let msg_count = msgs.len();
    let tid = thread.id.clone();
    let tid_log = tid.clone();

    // Link session file to this thread immediately so the next app restart can replay events.
    {
        let manager = state.shared_session_manager.clone();
        let session_id = id.clone();
        let link_tid = tid.clone();
        let link_result = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            let mut saved = manager.load_session(&session_id)?;
            saved.metadata.runtime_thread_id = Some(link_tid);
            manager.save_session(&saved).map(|_| ())
        })
        .await;
        match link_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                eprintln!("[resume-session] link runtime_thread_id: {e}");
            }
            Err(e) => {
                eprintln!("[resume-session] link runtime_thread_id task: {e}");
            }
        }
    }

    state
        .resume_tracker
        .register(&tid, &id, msg_count)
        .await;

    let tracker = state.resume_tracker.clone();
    let threads = state.runtime_threads.clone();
    tokio::spawn(async move {
        let _permit = tracker.acquire_seed_permit().await;

        let seed_result = tokio::task::spawn_blocking({
            let rt = tokio::runtime::Handle::current();
            let tid_clone = tid.clone();
            move || {
                rt.block_on(async {
                    threads
                        .seed_thread_from_messages(&tid_clone, &msgs)
                        .await
                })
            }
        })
        .await
        .map_err(|e| format!("seed panicked: {e}"))
        .and_then(|r| r.map_err(|e| format!("{e:#}")));

        match seed_result {
            Ok(()) => {
                let elapsed = t0.elapsed();
                eprintln!(
                    "[resume-session] seed done in {:.1}s total, thread={}",
                    elapsed.as_secs_f64(),
                    tid_log,
                );
                tracker.mark_ready(&tid).await;
            }
            Err(err) => {
                eprintln!(
                    "[resume-session] seed failed, thread={}: {}",
                    tid_log, err,
                );
                tracker.mark_error(&tid, err).await;
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(ResumeSessionResponse {
            thread_id: thread.id,
            session_id: id,
            message_count: msg_count,
            state: "seeding".to_string(),
        }),
    ))
}

async fn delete_session(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<StatusCode, ApiError> {
    let manager = state.shared_session_manager.clone();
    tokio::task::spawn_blocking({
        let id = id.clone();
        move || -> Result<(), std::io::Error> {
            manager.delete_session(&id)
        }
    })
    .await
    .map_err(|e| ApiError::internal(format!("delete session task panicked: {e}")))?
    .map_err(|e| map_session_err(&id, e, "delete"))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_resume_task(
    State(state): State<RuntimeApiState>,
    AxumPath(thread_id): AxumPath<String>,
) -> Result<Json<ResumeTaskState>, ApiError> {
    state
        .resume_tracker
        .get(&thread_id)
        .await
        .ok_or_else(|| ApiError::not_found(format!("resume task not found for thread {thread_id}")))
        .map(Json)
}

#[derive(Debug, Deserialize)]
struct PersistThreadSessionRequest {
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct PersistThreadSessionResponse {
    session_id: String,
    message_count: usize,
}

/// Writes the thread's turn/item history to `~/.deepseek/sessions/*.json` (same format as TUI).
async fn persist_thread_session(
    State(state): State<RuntimeApiState>,
    AxumPath(thread_id): AxumPath<String>,
    Json(req): Json<PersistThreadSessionRequest>,
) -> Result<Json<PersistThreadSessionResponse>, ApiError> {
    // All sync work — including the O(n) list_turns/list_items scans inside
    // export_thread_for_session_persist, plus serde + atomic write — runs on
    // the blocking thread pool so the /health endpoint stays responsive.
    let session = tokio::task::spawn_blocking({
        let threads = state.runtime_threads.clone();
        let manager = state.shared_session_manager.clone();
        let sid = req
            .session_id
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        move || -> Result<SavedSession, String> {
            let thread = threads
                .load_thread_sync(&thread_id)
                .map_err(|e| format!("thread not found: {e}"))?;

            let (messages, total_tokens) = threads
                .export_thread_for_session_persist(&thread_id)
                .map_err(|e| format!("Failed to export thread: {e}"))?;

            if sid.is_none() && messages.is_empty() {
                return Err(
                    "thread has no messages to persist; wait for turn.completed".to_string(),
                );
            }

            let sys = thread
                .system_prompt
                .as_ref()
                .map(|s| SystemPrompt::Text(s.clone()));

            if let Some(existing_id) = sid {
                let existing = manager
                    .load_session(&existing_id)
                    .map_err(|e| format!("read existing session: {e}"))?;
                let mut session =
                    update_session(existing, &messages, total_tokens, sys.as_ref());
                session.metadata.model = thread.model.clone();
                session.metadata.workspace = thread.workspace.clone();
                session.metadata.mode = Some(thread.mode.clone());
                if let Some(title) = &thread.title {
                    session.metadata.title = title.clone();
                }
                session.metadata.runtime_thread_id = Some(thread_id.clone());
                manager
                    .save_session(&session)
                    .map_err(|e| format!("save session: {e}"))?;
                Ok(session)
            } else {
                let mut session = create_saved_session_with_mode(
                    &messages,
                    &thread.model,
                    &thread.workspace,
                    total_tokens,
                    sys.as_ref(),
                    Some(thread.mode.as_str()),
                );
                if let Some(title) = &thread.title {
                    session.metadata.title = title.clone();
                }
                session.metadata.runtime_thread_id = Some(thread_id.clone());
                manager
                    .save_session(&session)
                    .map_err(|e| format!("save session: {e}"))?;
                Ok(session)
            }
        }
    })
    .await
    .map_err(|e| ApiError::internal(format!("persist session task panicked: {e}")))
    .and_then(|r| r.map_err(ApiError::internal))?;

    Ok(Json(PersistThreadSessionResponse {
        session_id: session.metadata.id.clone(),
        message_count: session.messages.len(),
    }))
}

fn session_to_detail(session: SavedSession) -> SessionDetailResponse {
    let messages: Vec<serde_json::Value> = session
        .messages
        .iter()
        .map(|msg| {
            let content_blocks: Vec<serde_json::Value> = msg
                .content
                .iter()
                .map(|block| match block {
                    crate::models::ContentBlock::Text { text, .. } => {
                        json!({ "type": "text", "text": text })
                    }
                    crate::models::ContentBlock::Thinking { thinking, .. } => {
                        json!({ "type": "thinking", "text": thinking })
                    }
                    _ => json!({ "type": "other" }),
                })
                .collect();
            json!({
                "role": msg.role,
                "content": content_blocks,
            })
        })
        .collect();
    SessionDetailResponse {
        metadata: session.metadata,
        messages,
        system_prompt: session.system_prompt,
    }
}

fn map_session_err(id: &str, err: std::io::Error, action: &str) -> ApiError {
    match err.kind() {
        std::io::ErrorKind::NotFound => ApiError::not_found(format!("Session '{id}' not found")),
        std::io::ErrorKind::InvalidData => {
            ApiError::bad_request(format!("Failed to parse session '{id}': {err}"))
        }
        std::io::ErrorKind::InvalidInput => {
            ApiError::bad_request(format!("Invalid session id '{id}'"))
        }
        _ => ApiError::internal(format!("Failed to {action} session '{id}': {err}")),
    }
}

async fn create_task(
    State(state): State<RuntimeApiState>,
    Json(mut req): Json<NewTaskRequest>,
) -> Result<(StatusCode, Json<TaskRecord>), ApiError> {
    if req.prompt.trim().is_empty() {
        return Err(ApiError::bad_request("prompt is required"));
    }
    if req.workspace.is_none() {
        req.workspace = Some(state.workspace.clone());
    }
    if req.model.is_none() {
        req.model = Some(
            state
                .config
                .default_text_model
                .clone()
                .unwrap_or_else(|| DEFAULT_TEXT_MODEL.to_string()),
        );
    }
    let task = state
        .task_manager
        .add_task(req)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(task)))
}

async fn create_thread(
    State(state): State<RuntimeApiState>,
    Json(mut req): Json<CreateThreadRequest>,
) -> Result<(StatusCode, Json<ThreadRecord>), ApiError> {
    if req.model.as_ref().is_none_or(|m| m.trim().is_empty()) {
        req.model = Some(
            state
                .config
                .default_text_model
                .clone()
                .unwrap_or_else(|| DEFAULT_TEXT_MODEL.to_string()),
        );
    }
    if req.workspace.is_none() {
        req.workspace = Some(state.workspace.clone());
    }
    if req.mode.as_ref().is_none_or(|m| m.trim().is_empty()) {
        req.mode = Some("agent".to_string());
    }

    let thread = state
        .runtime_threads
        .create_thread(req)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(thread)))
}

async fn list_threads(
    State(state): State<RuntimeApiState>,
    Query(query): Query<ThreadsQuery>,
) -> Result<Json<Vec<ThreadRecord>>, ApiError> {
    let filter = resolve_thread_filter(query.include_archived, query.archived_only);
    let threads = state
        .runtime_threads
        .list_threads(filter, query.limit)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(threads))
}

async fn list_threads_summary(
    State(state): State<RuntimeApiState>,
    Query(query): Query<ThreadSummaryQuery>,
) -> Result<Json<Vec<ThreadSummary>>, ApiError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let search = query.search.as_deref().map(str::to_ascii_lowercase);
    let filter = resolve_thread_filter(query.include_archived, query.archived_only);
    let threads = state
        .runtime_threads
        .list_threads(filter, Some(limit))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let mut summaries = Vec::new();
    for thread in threads {
        let detail = state
            .runtime_threads
            .get_thread_detail(&thread.id)
            .await
            .map_err(map_thread_err)?;
        let latest_turn = detail.turns.last();
        let latest_status =
            latest_turn.map(|turn| format!("{:?}", turn.status).to_ascii_lowercase());

        let title = thread
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| truncate_text(t, 72))
            .unwrap_or_else(|| {
                latest_turn
                    .map(|turn| {
                        if turn.input_summary.trim().is_empty() {
                            "New Thread".to_string()
                        } else {
                            truncate_text(&turn.input_summary, 72)
                        }
                    })
                    .unwrap_or_else(|| "New Thread".to_string())
            });

        let preview = detail
            .items
            .iter()
            .rev()
            .find_map(|item| match item.kind {
                TurnItemKind::AgentMessage | TurnItemKind::UserMessage => {
                    let text = item.detail.clone().unwrap_or_else(|| item.summary.clone());
                    if text.trim().is_empty() {
                        None
                    } else {
                        Some(truncate_text(&text, 140))
                    }
                }
                _ => None,
            })
            .unwrap_or_else(|| title.clone());

        if let Some(search) = &search {
            let haystack = format!(
                "{} {} {} {}",
                thread.id.to_ascii_lowercase(),
                title.to_ascii_lowercase(),
                preview.to_ascii_lowercase(),
                thread.model.to_ascii_lowercase()
            );
            if !haystack.contains(search) {
                continue;
            }
        }

        summaries.push(ThreadSummary {
            id: thread.id,
            title,
            preview,
            model: thread.model,
            mode: thread.mode,
            archived: thread.archived,
            updated_at: thread.updated_at,
            latest_turn_id: thread.latest_turn_id,
            latest_turn_status: latest_status,
        });
    }

    if summaries.len() > limit {
        summaries.truncate(limit);
    }

    Ok(Json(summaries))
}

async fn workspace_status(
    State(state): State<RuntimeApiState>,
) -> Result<Json<WorkspaceStatusResponse>, ApiError> {
    Ok(Json(collect_workspace_status(&state.workspace)))
}

// --- Thread workspace: snapshots, directory browse, text file read (desktop shell) ---

const MAX_SNAPSHOT_LIST: usize = 100;
const MAX_WORKSPACE_FILE_BYTES: usize = 512 * 1024;

#[derive(Debug, Deserialize)]
struct BrowseWorkspaceQuery {
    /// Path relative to thread workspace (`/` or `\` optional; no `..`).
    #[serde(default)]
    path: String,
}

/// Browse a directory on disk by absolute workspace root (Composer path before a runtime thread exists).
#[derive(Debug, Deserialize)]
struct BrowseWorkspaceByRootQuery {
    workspace: String,
    #[serde(default)]
    path: String,
}

#[derive(Debug, Deserialize)]
struct ReadWorkspaceFileByRootQuery {
    workspace: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct BrowseWorkspaceEntry {
    name: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
}

#[derive(Debug, Serialize)]
struct BrowseWorkspaceResponse {
    workspace: String,
    /// Relative path using `/` (empty = root).
    path: String,
    entries: Vec<BrowseWorkspaceEntry>,
}

#[derive(Debug, Serialize)]
struct SnapshotListEntryJson {
    /// 1-based, newest first (same as TUI `/restore N`).
    n: usize,
    id: String,
    label: String,
    timestamp: i64,
}

#[derive(Debug, Serialize)]
struct SnapshotsListResponse {
    workspace: String,
    snapshots: Vec<SnapshotListEntryJson>,
}

#[derive(Debug, Deserialize)]
struct RestoreSnapshotBody {
    /// 1-based index into newest-first list (`1` = latest).
    n: usize,
}

#[derive(Debug, Serialize)]
struct RestoreSnapshotResponse {
    restored: bool,
    label: String,
    id: String,
}

#[derive(Debug, Serialize)]
struct WorkspaceFileResponse {
    path: String,
    content: String,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    language_hint: Option<String>,
}

/// Create a missing workspace subdirectory before browse (e.g. Office `deliverables`).
fn ensure_workspace_browse_subdir(workspace_root: &Path, rel: &str) {
    let trimmed = rel.trim().trim_start_matches(['/', '\\']);
    if trimmed.is_empty() {
        return;
    }
    let rel_pb = PathBuf::from(trimmed);
    if rel_pb
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return;
    }
    let candidate = workspace_root.join(&rel_pb);
    if candidate.starts_with(workspace_root) && !candidate.exists() {
        let _ = std::fs::create_dir_all(&candidate);
    }
}

fn safe_thread_subpath(workspace_root: &Path, rel: &str) -> Result<PathBuf, ApiError> {
    let base = workspace_root
        .canonicalize()
        .map_err(|e| ApiError::bad_request(format!("workspace path: {e}")))?;
    let trimmed = rel.trim().trim_start_matches(['/', '\\']);
    if trimmed.is_empty() {
        return Ok(base);
    }
    let rel_pb = PathBuf::from(trimmed);
    if rel_pb.is_absolute() {
        return Err(ApiError::bad_request("path must be relative to workspace"));
    }
    for c in rel_pb.components() {
        if matches!(c, Component::ParentDir) {
            return Err(ApiError::bad_request("invalid path"));
        }
    }
    let candidate = base.join(&rel_pb);
    let canon = candidate
        .canonicalize()
        .map_err(|_| ApiError::not_found("path not found"))?;
    if !canon.starts_with(&base) {
        return Err(ApiError::forbidden("path outside workspace"));
    }
    Ok(canon)
}

/// Cap for git-aware suffix walks when resolving ambiguous model paths.
const WORKSPACE_RESOLVE_WALK_MAX: usize = 12_000;
const WORKSPACE_RESOLVE_MATCH_MAX: usize = 24;

fn normalize_workspace_rel_query(rel: &str) -> String {
    rel.trim()
        .trim_start_matches(['/', '\\'])
        .replace('\\', "/")
}

/// True when a suffix-only search is unlikely to explode (e.g. `mod.rs`).
fn workspace_suffix_walk_is_safe(suffix_norm: &str) -> bool {
    let parts: Vec<&str> = suffix_norm.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() >= 2 {
        return true;
    }
    if parts.len() == 1 {
        let name = parts[0];
        if matches!(
            name,
            "mod.rs" | "lib.rs" | "main.rs" | "index.ts" | "index.js" | "index.tsx" | "index.jsx"
        ) {
            return false;
        }
        return name.contains('.') && name.len() >= 5;
    }
    false
}

/// Resolve a workspace-relative file path for opens/preview: exact path, common `src/` omission,
/// then git-aware walk by path suffix (monorepo / wrong root segment).
fn resolve_existing_file_in_workspace(
    workspace_root: &Path,
    rel_raw: &str,
) -> Result<PathBuf, ApiError> {
    let n = normalize_workspace_rel_query(rel_raw);
    if n.is_empty() {
        return Err(ApiError::bad_request("path query required"));
    }
    if n.contains("..") {
        return Err(ApiError::bad_request("invalid path"));
    }

    let mut candidates: Vec<String> = Vec::new();
    candidates.push(n.clone());
    if !n.starts_with("src/") && !n.starts_with("crates/") && !n.starts_with("lib/") {
        candidates.push(format!("src/{n}"));
    }

    for c in &candidates {
        match safe_thread_subpath(workspace_root, c) {
            Ok(p) if p.is_file() => return Ok(p),
            Ok(_) => {}
            Err(e) => {
                if e.status == StatusCode::BAD_REQUEST {
                    return Err(e);
                }
            }
        }
    }

    if !workspace_suffix_walk_is_safe(&n) {
        return Err(ApiError::not_found("path not found"));
    }

    let base = workspace_root
        .canonicalize()
        .map_err(|e| ApiError::bad_request(format!("workspace: {e}")))?;
    let suffix_norm = n.trim_start_matches('/');
    let mut matches: Vec<PathBuf> = Vec::new();
    let walker = WalkBuilder::new(&base)
        .hidden(false)
        .git_ignore(true)
        .build();

    for (idx, entry) in walker.enumerate() {
        if idx > WORKSPACE_RESOLVE_WALK_MAX {
            break;
        }
        let entry = entry.map_err(|e| ApiError::internal(e.to_string()))?;
        let ft = entry.file_type();
        if !ft.map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(&base) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str == suffix_norm || rel_str.ends_with(&format!("/{suffix_norm}")) {
            matches.push(path.to_path_buf());
            if matches.len() > WORKSPACE_RESOLVE_MATCH_MAX {
                return Err(ApiError::not_found("path not found"));
            }
        }
    }

    match matches.len() {
        0 => Err(ApiError::not_found("path not found")),
        1 => Ok(matches[0].clone()),
        _ => {
            matches.sort_by_key(|p| {
                std::cmp::Reverse(
                    p.strip_prefix(&base)
                        .map(|x| x.components().count())
                        .unwrap_or(0),
                )
            });
            Ok(matches[0].clone())
        }
    }
}

fn workspace_relative_posix(ws: &Path, full: &Path) -> String {
    full.strip_prefix(ws)
        .map(|p| {
            p.iter()
                .filter_map(|s| s.to_str())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default()
}

fn read_dir_sorted(path: &Path) -> std::io::Result<Vec<BrowseWorkspaceEntry>> {
    let mut out = Vec::new();
    for ent in fs::read_dir(path)? {
        let ent = ent?;
        let name = ent.file_name().to_string_lossy().to_string();
        if name == ".git" && path.file_name().and_then(|n| n.to_str()) != Some(".git") {
            continue;
        }
        let meta = ent.metadata()?;
        if meta.is_dir() {
            out.push(BrowseWorkspaceEntry {
                name,
                kind: "directory".to_string(),
                size: None,
            });
        } else if meta.is_file() {
            out.push(BrowseWorkspaceEntry {
                name,
                kind: "file".to_string(),
                size: Some(meta.len()),
            });
        } else {
            continue;
        }
    }
    out.sort_by(|a, b| {
        let rank = |k: &str| match k {
            "directory" => 0,
            _ => 1,
        };
        rank(&a.kind)
            .cmp(&rank(&b.kind))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

fn language_from_name(name: &str) -> Option<String> {
    let ext = Path::new(name).extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "rs" => Some("rust".to_string()),
        "ts" | "tsx" => Some("typescript".to_string()),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript".to_string()),
        "md" | "mdx" => Some("markdown".to_string()),
        "json" => Some("json".to_string()),
        "toml" => Some("toml".to_string()),
        "yml" | "yaml" => Some("yaml".to_string()),
        "css" => Some("css".to_string()),
        "html" | "htm" => Some("html".to_string()),
        "py" => Some("python".to_string()),
        "sh" | "bash" => Some("bash".to_string()),
        "go" => Some("go".to_string()),
        "c" | "h" => Some("c".to_string()),
        "cpp" | "cc" | "hpp" => Some("cpp".to_string()),
        _ => None,
    }
}

async fn list_thread_snapshots(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<SnapshotsListResponse>, ApiError> {
    let detail = state
        .runtime_threads
        .get_thread_detail(&id)
        .await
        .map_err(map_thread_err)?;
    let workspace_display = detail.thread.workspace.display().to_string();
    let ws = detail.thread.workspace.clone();
    let limit = q
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(50)
        .clamp(1, MAX_SNAPSHOT_LIST);
    let snapshots = tokio::task::spawn_blocking(move || {
        let repo = SnapshotRepo::open_or_init(&ws)?;
        repo.list(limit)
    })
    .await
    .map_err(|e| ApiError::internal(format!("snapshot task: {e}")))?
    .map_err(|e| {
        ApiError::bad_request(format!(
            "snapshots unavailable for {}: {e}",
            workspace_display
        ))
    })?;
    let mut entries = Vec::with_capacity(snapshots.len());
    for (i, s) in snapshots.iter().enumerate() {
        entries.push(SnapshotListEntryJson {
            n: i + 1,
            id: s.id.as_str().to_string(),
            label: s.label.clone(),
            timestamp: s.timestamp,
        });
    }
    Ok(Json(SnapshotsListResponse {
        workspace: workspace_display,
        snapshots: entries,
    }))
}

async fn restore_thread_snapshot(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<RestoreSnapshotBody>,
) -> Result<Json<RestoreSnapshotResponse>, ApiError> {
    if body.n < 1 {
        return Err(ApiError::bad_request("n must be >= 1"));
    }
    let detail = state
        .runtime_threads
        .get_thread_detail(&id)
        .await
        .map_err(map_thread_err)?;
    if !detail.thread.trust_mode {
        return Err(ApiError::forbidden(
            "restore requires trust_mode on this thread (PATCH /v1/threads/{id} with {\"trust_mode\": true} or use TUI `/trust on`).",
        ));
    }
    let ws = detail.thread.workspace.clone();
    let n = body.n;
    let restored = tokio::task::spawn_blocking(move || {
        let repo = SnapshotRepo::open_or_init(&ws)?;
        let snaps = repo.list(MAX_SNAPSHOT_LIST)?;
        if n > snaps.len() {
            return Err(ApiError::bad_request(format!(
                "only {} snapshot(s); n={n} out of range",
                snaps.len()
            )));
        }
        let target = &snaps[n - 1];
        let id_snap = target.id.clone();
        let label = target.label.clone();
        repo.restore(&id_snap)?;
        Ok::<_, ApiError>((label, id_snap))
    })
    .await
    .map_err(|e| ApiError::internal(format!("restore task: {e}")))??;
    Ok(Json(RestoreSnapshotResponse {
        restored: true,
        label: restored.0,
        id: restored.1.as_str().to_string(),
    }))
}

async fn browse_thread_workspace(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<BrowseWorkspaceQuery>,
) -> Result<Json<BrowseWorkspaceResponse>, ApiError> {
    let detail = state
        .runtime_threads
        .get_thread_detail(&id)
        .await
        .map_err(map_thread_err)?;
    let ws_path = detail.thread.workspace.as_path();
    let base = ws_path
        .canonicalize()
        .map_err(|e| ApiError::bad_request(format!("workspace: {e}")))?;
    ensure_workspace_browse_subdir(&base, &q.path);
    let dir_path = safe_thread_subpath(ws_path, &q.path)?;
    if !dir_path.is_dir() {
        return Err(ApiError::bad_request("not a directory"));
    }
    let dir_for_rel = dir_path.clone();
    let entries = tokio::task::spawn_blocking(move || read_dir_sorted(&dir_path))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let rel = workspace_relative_posix(&base, &dir_for_rel);
    Ok(Json(BrowseWorkspaceResponse {
        workspace: path_for_api_display(&base),
        path: rel,
        entries,
    }))
}

async fn browse_workspace_by_root(
    Query(q): Query<BrowseWorkspaceByRootQuery>,
) -> Result<Json<BrowseWorkspaceResponse>, ApiError> {
    let ws_raw = q.workspace.trim();
    if ws_raw.is_empty() {
        return Err(ApiError::bad_request("workspace query required"));
    }
    let ws_path = PathBuf::from(ws_raw);
    let base = ws_path
        .canonicalize()
        .map_err(|e| ApiError::bad_request(format!("workspace: {e}")))?;
    if !base.is_dir() {
        return Err(ApiError::bad_request("workspace is not a directory"));
    }
    ensure_workspace_browse_subdir(&base, &q.path);
    let dir_path = safe_thread_subpath(&base, &q.path)?;
    if !dir_path.is_dir() {
        return Err(ApiError::bad_request("not a directory"));
    }
    let dir_for_rel = dir_path.clone();
    let entries = tokio::task::spawn_blocking(move || read_dir_sorted(&dir_path))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let rel = workspace_relative_posix(&base, &dir_for_rel);
    Ok(Json(BrowseWorkspaceResponse {
        workspace: path_for_api_display(&base),
        path: rel,
        entries,
    }))
}

fn path_for_api_display(path: &Path) -> String {
    let s = path.display().to_string();
    #[cfg(windows)]
    {
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    s
}

async fn read_workspace_file_by_root(
    Query(q): Query<ReadWorkspaceFileByRootQuery>,
) -> Result<Json<WorkspaceFileResponse>, ApiError> {
    let ws_raw = q.workspace.trim();
    if ws_raw.is_empty() {
        return Err(ApiError::bad_request("workspace query required"));
    }
    let ws_path = PathBuf::from(ws_raw);
    let base = ws_path
        .canonicalize()
        .map_err(|e| ApiError::bad_request(format!("workspace: {e}")))?;
    if !base.is_dir() {
        return Err(ApiError::bad_request("workspace is not a directory"));
    }
    if q.path.trim().is_empty() {
        return Err(ApiError::bad_request("path query required"));
    }
    let file_path = resolve_existing_file_in_workspace(&base, &q.path)?;
    let rel = workspace_relative_posix(&base, &file_path);
    let path_clone = file_path.clone();
    let (content, truncated) = tokio::task::spawn_blocking(move || {
        let meta =
            fs::metadata(&path_clone).map_err(|e| ApiError::internal(format!("metadata: {e}")))?;
        let len = meta.len() as usize;
        if len > MAX_WORKSPACE_FILE_BYTES {
            return Err(ApiError::bad_request(format!(
                "file too large (max {MAX_WORKSPACE_FILE_BYTES} bytes)"
            )));
        }
        let bytes = fs::read(&path_clone).map_err(|e| ApiError::internal(e.to_string()))?;
        let text = String::from_utf8(bytes).map_err(|_| {
            ApiError::bad_request("file is not UTF-8 text; binary preview not supported")
        })?;
        Ok::<_, ApiError>((text, false))
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))??;
    let name = file_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let language_hint = language_from_name(&name);
    Ok(Json(WorkspaceFileResponse {
        path: rel,
        content,
        truncated,
        language_hint,
    }))
}

async fn read_thread_workspace_file(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<BrowseWorkspaceQuery>,
) -> Result<Json<WorkspaceFileResponse>, ApiError> {
    let detail = state
        .runtime_threads
        .get_thread_detail(&id)
        .await
        .map_err(map_thread_err)?;
    let ws = detail.thread.workspace.clone();
    let base = ws
        .canonicalize()
        .map_err(|e| ApiError::bad_request(format!("workspace: {e}")))?;
    if q.path.trim().is_empty() {
        return Err(ApiError::bad_request("path query required"));
    }
    let file_path = resolve_existing_file_in_workspace(&ws, &q.path)?;
    let rel = workspace_relative_posix(&base, &file_path);
    let path_clone = file_path.clone();
    let (content, truncated) = tokio::task::spawn_blocking(move || {
        let meta =
            fs::metadata(&path_clone).map_err(|e| ApiError::internal(format!("metadata: {e}")))?;
        let len = meta.len() as usize;
        if len > MAX_WORKSPACE_FILE_BYTES {
            return Err(ApiError::bad_request(format!(
                "file too large (max {MAX_WORKSPACE_FILE_BYTES} bytes)"
            )));
        }
        let bytes = fs::read(&path_clone).map_err(|e| ApiError::internal(e.to_string()))?;
        let text = String::from_utf8(bytes).map_err(|_| {
            ApiError::bad_request("file is not UTF-8 text; binary preview not supported")
        })?;
        Ok::<_, ApiError>((text, false))
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))??;
    let name = file_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let language_hint = language_from_name(&name);
    Ok(Json(WorkspaceFileResponse {
        path: rel,
        content,
        truncated,
        language_hint,
    }))
}

async fn list_skills(
    State(state): State<RuntimeApiState>,
) -> Result<Json<SkillsResponse>, ApiError> {
    let skills_dir = resolve_skills_dir(&state.config, &state.workspace);
    let registry = SkillRegistry::discover(&skills_dir);
    let skills = registry
        .list()
        .iter()
        .map(|skill| SkillEntry {
            name: skill.name.clone(),
            description: skill.description.clone(),
            path: skills_dir.join(&skill.name).join("SKILL.md"),
        })
        .collect();
    Ok(Json(SkillsResponse {
        directory: skills_dir,
        warnings: registry.warnings().to_vec(),
        skills,
    }))
}

async fn create_skill(
    State(state): State<RuntimeApiState>,
    Json(req): Json<CreateSkillRequest>,
) -> Result<(StatusCode, Json<CreateSkillResponse>), ApiError> {
    let name = validate_skill_directory_name(&req.name)?;
    let name_for_task = name.clone();
    let config = state.config.clone();
    let workspace = state.workspace.clone();
    let parent_directory = req.parent_directory.clone();
    let scope = req.scope.clone();

    let (skills_root_used, skill_md_path, warnings) = tokio::task::spawn_blocking(move || {
        let root =
            resolve_create_skill_parent(&config, &workspace, parent_directory.as_ref(), &scope)?;
        fs::create_dir_all(&root).map_err(|e| {
            ApiError::internal(format!(
                "failed to create skills directory {}: {e}",
                root.display()
            ))
        })?;

        let skill_dir = root.join(&name_for_task);
        if skill_dir.exists() {
            return Err(ApiError::conflict(format!(
                "skill directory already exists: {}",
                skill_dir.display()
            )));
        }
        fs::create_dir_all(&skill_dir)
            .map_err(|e| ApiError::internal(format!("failed to create skill directory: {e}")))?;
        let md_path = skill_dir.join("SKILL.md");
        if md_path.exists() {
            return Err(ApiError::conflict("SKILL.md already exists".to_string()));
        }
        let body = skill_md_template(&name_for_task);
        fs::write(&md_path, body).map_err(|e| ApiError::internal(e.to_string()))?;

        let registry = SkillRegistry::discover(&root);
        let warnings = registry.warnings().to_vec();
        Ok::<_, ApiError>((root, md_path, warnings))
    })
    .await
    .map_err(|e| ApiError::internal(format!("create skill task: {e}")))??;

    let list_directory = resolve_skills_dir(&state.config, &state.workspace);
    let reg_list = SkillRegistry::discover(&skills_root_used);
    let description = reg_list
        .get(&name)
        .map(|s| s.description.clone())
        .unwrap_or_else(|| "Describe what this skill does.".to_string());

    let skill_entry = SkillEntry {
        name,
        description,
        path: skill_md_path,
    };

    Ok((
        StatusCode::CREATED,
        Json(CreateSkillResponse {
            skill: skill_entry,
            directory: list_directory,
            skills_root: skills_root_used,
            warnings,
        }),
    ))
}

async fn import_skill_local(
    State(state): State<RuntimeApiState>,
    Json(req): Json<ImportSkillLocalRequest>,
) -> Result<(StatusCode, Json<CreateSkillResponse>), ApiError> {
    if req.source_directory.as_os_str().is_empty() {
        return Err(ApiError::bad_request("source_directory is required"));
    }
    let config = state.config.clone();
    let workspace = state.workspace.clone();
    let parent_directory = req.parent_directory.clone();
    let scope = req.scope.clone();
    let source_directory = req.source_directory.clone();
    let replace = req.replace;

    let (skills_root_used, installed) = tokio::task::spawn_blocking(move || {
        let root =
            resolve_create_skill_parent(&config, &workspace, parent_directory.as_ref(), &scope)?;
        fs::create_dir_all(&root).map_err(|e| {
            ApiError::internal(format!(
                "failed to create skills directory {}: {e}",
                root.display()
            ))
        })?;
        let installed = import_local_directory(
            &source_directory,
            &root,
            replace,
            DEFAULT_MAX_SIZE_BYTES,
        )
        .map_err(map_skill_install_api_error)?;
        Ok::<_, ApiError>((root, installed))
    })
    .await
    .map_err(|e| ApiError::internal(format!("import skill task: {e}")))??;

    build_skill_mutation_response(
        &state,
        &skills_root_used,
        &installed.name,
        installed.path.join("SKILL.md"),
    )
    .await
    .map(|json| (StatusCode::CREATED, json))
}

async fn install_skill_remote(
    State(state): State<RuntimeApiState>,
    Json(req): Json<InstallSkillRemoteRequest>,
) -> Result<(StatusCode, Json<CreateSkillResponse>), ApiError> {
    let spec = req.spec.trim();
    if spec.is_empty() {
        return Err(ApiError::bad_request("spec is required"));
    }
    let source = InstallSource::parse(spec)
        .map_err(|e| ApiError::bad_request(format!("invalid install spec: {e}")))?;

    let config = state.config.clone();
    let workspace = state.workspace.clone();
    let parent_directory = req.parent_directory;
    let scope = req.scope;
    let replace = req.replace;
    let skills_cfg = config.skills.as_ref();
    let registry_url = skills_cfg
        .map(crate::config::SkillsConfig::registry_url)
        .unwrap_or_else(|| install::DEFAULT_REGISTRY_URL.to_string());
    let max_size = skills_cfg
        .map(crate::config::SkillsConfig::max_install_size_bytes)
        .unwrap_or(DEFAULT_MAX_SIZE_BYTES);
    let network = config
        .network
        .clone()
        .unwrap_or_default()
        .into_runtime();

    let skills_root = tokio::task::spawn_blocking({
        let config = config.clone();
        let workspace = workspace.clone();
        move || {
            resolve_create_skill_parent(&config, &workspace, parent_directory.as_ref(), &scope)
        }
    })
    .await
    .map_err(|e| ApiError::internal(format!("resolve skills root: {e}")))??;

    fs::create_dir_all(&skills_root).map_err(|e| {
        ApiError::internal(format!(
            "failed to create skills directory {}: {e}",
            skills_root.display()
        ))
    })?;

    let outcome = install::install_with_registry(
        source,
        &skills_root,
        max_size,
        &network,
        replace,
        &registry_url,
    )
    .await
    .map_err(|e| ApiError::internal(format!("skill install: {e:#}")))?;

    let installed = match outcome {
        InstallOutcome::Installed(skill) => skill,
        InstallOutcome::NeedsApproval(host) => {
            return Err(ApiError::forbidden(format!(
                "network host '{host}' requires approval; add it to [network] allow in config.toml and retry"
            )));
        }
        InstallOutcome::NetworkDenied(host) => {
            return Err(ApiError::forbidden(format!(
                "network host '{host}' is denied by policy"
            )));
        }
    };

    build_skill_mutation_response(
        &state,
        &skills_root,
        &installed.name,
        installed.path.join("SKILL.md"),
    )
    .await
    .map(|json| (StatusCode::CREATED, json))
}

async fn build_skill_mutation_response(
    state: &RuntimeApiState,
    skills_root_used: &Path,
    skill_name: &str,
    skill_md_path: PathBuf,
) -> Result<Json<CreateSkillResponse>, ApiError> {
    let list_directory = resolve_skills_dir(&state.config, &state.workspace);
    let reg_list = SkillRegistry::discover(skills_root_used);
    let description = reg_list
        .get(skill_name)
        .map(|s| s.description.clone())
        .unwrap_or_else(|| "Describe what this skill does.".to_string());
    let warnings = reg_list.warnings().to_vec();
    Ok(Json(CreateSkillResponse {
        skill: SkillEntry {
            name: skill_name.to_string(),
            description,
            path: skill_md_path,
        },
        directory: list_directory,
        skills_root: skills_root_used.to_path_buf(),
        warnings,
    }))
}

fn map_skill_install_api_error(err: anyhow::Error) -> ApiError {
    if let Some(InstallError::AlreadyInstalled(name)) = err.downcast_ref() {
        return ApiError::conflict(format!("skill already installed: {name} (set replace=true to overwrite)"));
    }
    ApiError::bad_request(format!("{err:#}"))
}

async fn list_mcp_servers(
    State(state): State<RuntimeApiState>,
) -> Result<Json<McpServersResponse>, ApiError> {
    let config = load_mcp_config_or_default(&state.mcp_config_path)?;
    let mut pool = McpPool::new(config.clone());
    let connected: HashSet<String> = if config.servers.is_empty() {
        HashSet::new()
    } else {
        match tokio::time::timeout(Duration::from_secs(2), pool.connect_all()).await {
            Ok(_) => pool
                .connected_servers()
                .into_iter()
                .map(str::to_string)
                .collect(),
            Err(_elapsed) => HashSet::new(),
        }
    };

    let mut servers = Vec::new();
    for (name, server_cfg) in config.servers {
        servers.push(McpServerEntry {
            name: name.clone(),
            enabled: server_cfg.is_enabled(),
            required: server_cfg.required,
            command: server_cfg.command.clone(),
            url: server_cfg.url.clone(),
            args: server_cfg.args.clone(),
            connected: connected.contains(&name),
            enabled_tools: server_cfg.enabled_tools.clone(),
            disabled_tools: server_cfg.disabled_tools.clone(),
        });
    }
    servers.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Json(McpServersResponse { servers }))
}

async fn merge_mcp_config_json(
    State(state): State<RuntimeApiState>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let s = std::str::from_utf8(&body).map_err(|_| ApiError::bad_request("请求体须为 UTF-8"))?;
    let merged = crate::mcp::merge_mcp_json_fragment(&state.mcp_config_path, s)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "merged_servers": merged,
        })),
    ))
}

async fn add_mcp_server(
    State(state): State<RuntimeApiState>,
    Json(req): Json<McpAddServerRequest>,
) -> Result<StatusCode, ApiError> {
    crate::mcp::add_server_config(
        &state.mcp_config_path,
        req.name,
        req.command,
        req.url,
        req.args,
    )
    .map_err(|e| ApiError::bad_request(format!("添加 MCP 服务器失败：{e}")))?;
    Ok(StatusCode::CREATED)
}

async fn get_mcp_server(
    State(state): State<RuntimeApiState>,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<McpServerConfig>, ApiError> {
    let entry = crate::mcp::get_server_entry(&state.mcp_config_path, &name)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let Some(cfg) = entry else {
        return Err(ApiError::not_found(format!(
            "MCP server '{name}' not found"
        )));
    };
    Ok(Json(cfg))
}

async fn update_mcp_server(
    State(state): State<RuntimeApiState>,
    AxumPath(name): AxumPath<String>,
    Json(cfg): Json<McpServerConfig>,
) -> Result<Json<Value>, ApiError> {
    crate::mcp::replace_server_in_config(&state.mcp_config_path, &name, cfg)
        .map_err(|e| ApiError::bad_request(format!("更新 MCP 服务器失败：{e}")))?;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_mcp_server(
    State(state): State<RuntimeApiState>,
    AxumPath(name): AxumPath<String>,
) -> Result<StatusCode, ApiError> {
    crate::mcp::remove_server_from_config(&state.mcp_config_path, &name)
        .map_err(|e| ApiError::bad_request(format!("删除 MCP 服务器失败：{e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_mcp_tools(
    State(state): State<RuntimeApiState>,
    Query(query): Query<McpToolsQuery>,
) -> Result<Json<McpToolsResponse>, ApiError> {
    let mut pool = McpPool::from_config_path(&state.mcp_config_path)
        .map_err(|e| ApiError::internal(format!("Failed to load MCP config: {e}")))?;
    let _ = tokio::time::timeout(Duration::from_secs(2), pool.connect_all()).await;

    let mut tools = Vec::new();
    for (prefixed_name, tool) in pool.all_tools() {
        let Some(rest) = prefixed_name.strip_prefix("mcp_") else {
            continue;
        };
        let Some((server, name)) = rest.split_once('_') else {
            continue;
        };

        if let Some(filter) = query.server.as_deref()
            && server != filter
        {
            continue;
        }

        tools.push(McpToolEntry {
            server: server.to_string(),
            name: name.to_string(),
            prefixed_name,
            description: tool.description.clone(),
            input_schema: tool.input_schema.clone(),
        });
    }

    tools.sort_by(|a, b| a.server.cmp(&b.server).then_with(|| a.name.cmp(&b.name)));

    Ok(Json(McpToolsResponse { tools }))
}

async fn list_automations(
    State(state): State<RuntimeApiState>,
) -> Result<Json<Vec<AutomationRecord>>, ApiError> {
    let manager = state.automations.lock().await;
    let automations = manager
        .list_automations()
        .map_err(|e| ApiError::internal(format!("Failed to list automations: {e}")))?;
    Ok(Json(automations))
}

async fn create_automation(
    State(state): State<RuntimeApiState>,
    Json(req): Json<CreateAutomationRequest>,
) -> Result<(StatusCode, Json<AutomationRecord>), ApiError> {
    let manager = state.automations.lock().await;
    let automation = manager
        .create_automation(req)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(automation)))
}

async fn get_automation(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<AutomationRecord>, ApiError> {
    let manager = state.automations.lock().await;
    let automation = manager.get_automation(&id).map_err(map_automation_err)?;
    Ok(Json(automation))
}

async fn update_automation(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateAutomationRequest>,
) -> Result<Json<AutomationRecord>, ApiError> {
    let manager = state.automations.lock().await;
    let automation = manager
        .update_automation(&id, req)
        .map_err(map_automation_err)?;
    Ok(Json(automation))
}

async fn delete_automation(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<AutomationRecord>, ApiError> {
    let manager = state.automations.lock().await;
    let automation = manager.delete_automation(&id).map_err(map_automation_err)?;
    Ok(Json(automation))
}

async fn run_automation(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<AutomationRunRecord>, ApiError> {
    let manager = state.automations.lock().await;
    let run = manager
        .run_now(&id, &state.task_manager)
        .await
        .map_err(map_automation_err)?;
    Ok(Json(run))
}

async fn pause_automation(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<AutomationRecord>, ApiError> {
    let manager = state.automations.lock().await;
    let automation = manager.pause_automation(&id).map_err(map_automation_err)?;
    Ok(Json(automation))
}

async fn resume_automation(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<AutomationRecord>, ApiError> {
    let manager = state.automations.lock().await;
    let automation = manager.resume_automation(&id).map_err(map_automation_err)?;
    Ok(Json(automation))
}

async fn list_automation_runs(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<AutomationRunsQuery>,
) -> Result<Json<Vec<AutomationRunRecord>>, ApiError> {
    let manager = state.automations.lock().await;
    let runs = manager
        .list_runs(&id, query.limit)
        .map_err(map_automation_err)?;
    Ok(Json(runs))
}

async fn get_thread(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<ThreadDetail>, ApiError> {
    let detail = state
        .runtime_threads
        .get_thread_detail(&id)
        .await
        .map_err(map_thread_err)?;
    Ok(Json(detail))
}

async fn update_thread(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateThreadRequest>,
) -> Result<Json<ThreadRecord>, ApiError> {
    let thread = state
        .runtime_threads
        .update_thread(&id, req)
        .await
        .map_err(map_thread_err)?;
    Ok(Json(thread))
}

async fn resume_thread(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<ThreadRecord>, ApiError> {
    let thread = state
        .runtime_threads
        .resume_thread(&id)
        .await
        .map_err(map_thread_err)?;
    Ok(Json(thread))
}

async fn fork_thread(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<(StatusCode, Json<ThreadRecord>), ApiError> {
    let thread = state
        .runtime_threads
        .fork_thread(&id)
        .await
        .map_err(map_thread_err)?;
    Ok((StatusCode::CREATED, Json(thread)))
}

async fn start_thread_turn(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<StartTurnRequest>,
) -> Result<(StatusCode, Json<StartTurnResponse>), ApiError> {
    let turn = state
        .runtime_threads
        .start_turn(&id, req)
        .await
        .map_err(map_thread_err)?;
    let thread = state
        .runtime_threads
        .get_thread(&id)
        .await
        .map_err(map_thread_err)?;
    Ok((
        StatusCode::CREATED,
        Json(StartTurnResponse { thread, turn }),
    ))
}

async fn steer_thread_turn(
    State(state): State<RuntimeApiState>,
    AxumPath((id, turn_id)): AxumPath<(String, String)>,
    Json(req): Json<SteerTurnRequest>,
) -> Result<Json<TurnRecord>, ApiError> {
    let turn = state
        .runtime_threads
        .steer_turn(&id, &turn_id, req)
        .await
        .map_err(map_thread_err)?;
    Ok(Json(turn))
}

async fn resolve_approval(
    State(state): State<RuntimeApiState>,
    AxumPath((id, turn_id)): AxumPath<(String, String)>,
    Json(req): Json<ResolveApprovalRequest>,
) -> Result<Json<Value>, ApiError> {
    let approved = match req.decision.as_str() {
        "approve" => true,
        "deny" => false,
        _ => {
            return Err(ApiError::bad_request(
                "decision must be 'approve' or 'deny'",
            ));
        }
    };
    state
        .runtime_threads
        .resolve_approval(&id, &turn_id, &req.tool_call_id, approved)
        .await
        .map_err(map_thread_err)?;
    Ok(Json(json!({
        "ok": true,
        "tool_call_id": req.tool_call_id,
        "decision": req.decision,
        "turn_id": turn_id,
    })))
}

async fn interrupt_thread_turn(
    State(state): State<RuntimeApiState>,
    AxumPath((id, turn_id)): AxumPath<(String, String)>,
) -> Result<Json<TurnRecord>, ApiError> {
    let turn = state
        .runtime_threads
        .interrupt_turn(&id, &turn_id)
        .await
        .map_err(map_thread_err)?;
    Ok(Json(turn))
}

async fn compact_thread(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<CompactThreadRequest>,
) -> Result<(StatusCode, Json<StartTurnResponse>), ApiError> {
    let turn = state
        .runtime_threads
        .compact_thread(&id, req)
        .await
        .map_err(map_thread_err)?;
    let thread = state
        .runtime_threads
        .get_thread(&id)
        .await
        .map_err(map_thread_err)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(StartTurnResponse { thread, turn }),
    ))
}

async fn get_thread_checklist(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    match state.runtime_threads.get_thread_checklist(&id) {
        Some(json_str) => {
            let parsed: Value =
                serde_json::from_str(&json_str).unwrap_or(Value::Null);
            Ok(Json(parsed))
        }
        None => Ok(Json(Value::Null)),
    }
}

async fn get_thread_scratchpad_status(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let status = state
        .runtime_threads
        .get_thread_scratchpad_status(&id)
        .map_err(map_thread_err)?;
    Ok(Json(status.unwrap_or(Value::Null)))
}

async fn get_thread_context(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<crate::context_snapshot::ThreadContextSnapshot>, ApiError> {
    let snapshot = state
        .runtime_threads
        .get_thread_context(&id)
        .await
        .map_err(map_thread_err)?;
    Ok(Json(snapshot))
}

async fn list_tasks(
    State(state): State<RuntimeApiState>,
    Query(query): Query<TasksQuery>,
) -> Result<Json<TasksResponse>, ApiError> {
    let tasks = state.task_manager.list_tasks(query.limit).await;
    let counts = state.task_manager.counts().await;
    Ok(Json(TasksResponse { tasks, counts }))
}

async fn get_task(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<TaskRecord>, ApiError> {
    let task = state
        .task_manager
        .get_task(&id)
        .await
        .map_err(map_task_err)?;
    Ok(Json(task))
}

async fn cancel_task(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<TaskRecord>, ApiError> {
    let task = state
        .task_manager
        .cancel_task(&id)
        .await
        .map_err(map_task_err)?;
    Ok(Json(task))
}

async fn stream_thread_events(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<ThreadEventsQuery>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<SseEvent, Infallible>>>, ApiError> {
    let _ = state
        .runtime_threads
        .get_thread(&id)
        .await
        .map_err(map_thread_err)?;

    let backlog = state
        .runtime_threads
        .events_since(&id, query.since_seq)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let mut last_seq = query.since_seq.unwrap_or(0);
    if let Some(last) = backlog.last() {
        last_seq = last.seq;
    }

    let replay_only = query.replay_only.unwrap_or(false);
    let mut live = state.runtime_threads.subscribe_events();
    let thread_id = id.clone();
    let stream = stream! {
        for event in backlog {
            let seq = event.seq;
            let event_name = event.event.clone();
            let payload = runtime_event_payload(event);
            yield Ok(sse_json_seq(seq, &event_name, payload));
        }
        if replay_only {
            return;
        }
        loop {
            let incoming = live.recv().await;
            let Ok(event) = incoming else {
                break;
            };
            if event.thread_id != thread_id {
                continue;
            }
            if event.seq <= last_seq {
                continue;
            }
            last_seq = event.seq;
            let seq = event.seq;
            let event_name = event.event.clone();
            let payload = runtime_event_payload(event);
            yield Ok(sse_json_seq(seq, &event_name, payload));
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

async fn stream_turn(
    State(state): State<RuntimeApiState>,
    Json(req): Json<StreamTurnRequest>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<SseEvent, Infallible>>>, ApiError> {
    if req.prompt.trim().is_empty() {
        return Err(ApiError::bad_request("prompt is required"));
    }

    let model = req.model.clone().unwrap_or_else(|| {
        state
            .config
            .default_text_model
            .clone()
            .unwrap_or_else(|| DEFAULT_TEXT_MODEL.to_string())
    });
    let workspace = req
        .workspace
        .clone()
        .unwrap_or_else(|| state.workspace.clone());
    let mode = req.mode.clone().unwrap_or_else(|| "agent".to_string());
    let allow_shell = req.allow_shell.unwrap_or(state.config.allow_shell());
    let trust_mode = req.trust_mode.unwrap_or(false);
    let auto_approve = req.auto_approve.unwrap_or(false);
    let prompt = req.prompt;
    let task_type = crate::task_type::resolve_task_type(
        req.task_type.as_deref(),
        &workspace,
        Some(prompt.as_str()),
    );

    let thread = state
        .runtime_threads
        .create_thread(CreateThreadRequest {
            model: Some(model.clone()),
            workspace: Some(workspace.clone()),
            mode: Some(mode.clone()),
            allow_shell: Some(allow_shell),
            trust_mode: Some(trust_mode),
            auto_approve: Some(auto_approve),
            archived: true,
            system_prompt: None,
            task_id: None,
            task_type: Some(task_type.as_str().to_string()),
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create stream thread: {e}")))?;

    let turn = state
        .runtime_threads
        .start_turn(
            &thread.id,
            StartTurnRequest {
                prompt,
                input_summary: None,
                model: Some(model.clone()),
                mode: Some(mode.clone()),
                allow_shell: Some(allow_shell),
                trust_mode: Some(trust_mode),
                auto_approve: Some(auto_approve),
                route_intent: req.route_intent.clone(),
            },
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to start stream turn: {e}")))?;

    let backlog = state
        .runtime_threads
        .events_since(&thread.id, None)
        .map_err(|e| ApiError::internal(format!("Failed to load stream backlog: {e}")))?;
    let mut live = state.runtime_threads.subscribe_events();
    let thread_id = thread.id.clone();
    let turn_id = turn.id.clone();

    let stream = stream! {
        yield Ok(sse_json("turn.started", json!({
            "thread_id": thread.id,
            "turn_id": turn.id,
            "model": model,
            "mode": mode,
            "workspace": workspace,
        })));

        for event in backlog {
            if event.thread_id != thread_id || event.turn_id.as_deref() != Some(&turn_id) {
                continue;
            }
            if let Some(mapped) = map_compat_stream_event(&event) {
                yield Ok(mapped);
            }
            if event.event == "turn.completed" {
                yield Ok(sse_json("done", json!({})));
                return;
            }
        }

        loop {
            let incoming = live.recv().await;
            let Ok(event) = incoming else {
                yield Ok(sse_json("error", json!({ "message": "event channel closed" })));
                break;
            };
            if event.thread_id != thread_id || event.turn_id.as_deref() != Some(&turn_id) {
                continue;
            }
            if let Some(mapped) = map_compat_stream_event(&event) {
                yield Ok(mapped);
            }
            if event.event == "turn.completed" {
                break;
            }
        }

        yield Ok(sse_json("done", json!({})));
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

fn runtime_event_payload(event: crate::runtime_threads::RuntimeEventRecord) -> serde_json::Value {
    json!({
        "seq": event.seq,
        "timestamp": event.timestamp,
        "thread_id": event.thread_id,
        "turn_id": event.turn_id,
        "item_id": event.item_id,
        "event": event.event,
        "payload": event.payload,
    })
}

/// Map a raw runtime event to a compat SSE event name and payload.
///
/// # SSE event names (stable compat surface):
///
/// | SSE `event`      | Source                 | Description               |
/// |------------------|------------------------|---------------------------|
/// | `thinking.delta` | `item.delta` (thinking) | Reasoning / thinking increment |
/// | `message.delta`  | `item.delta` (agent)   | Assistant text increment  |
/// | `tool.progress`  | `item.delta` (tool)    | Tool output streaming     |
/// | `tool.started`   | `item.started` (tool)  | Tool call began           |
/// | `tool.completed` | `item.completed` (tool)| Tool call finished        |
/// | `status`         | `item.completed`       | Status/notification       |
/// | `error`          | `item.completed`       | Error message             |
/// | `approval.required` | `approval.required` | Exec approval needed      |
/// | `sandbox.denied` | `sandbox.denied`       | Sandbox policy denied     |
/// | `turn.completed` | `turn.completed`       | Turn ended with usage     |
///
/// Additional events emitted outside this function by stream_turn:
/// - `turn.started` — emitted as first frame
/// - `done` — emitted as final frame to close SSE
fn map_compat_stream_event(event: &crate::runtime_threads::RuntimeEventRecord) -> Option<SseEvent> {
    let payload = &event.payload;
    match event.event.as_str() {
        "item.delta" => {
            let kind = payload
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if kind == "agent_message" {
                let content = payload
                    .get("delta")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                Some(sse_json_seq(
                    event.seq,
                    "message.delta",
                    json!({ "content": content }),
                ))
            } else if kind == "thinking" {
                let content = payload
                    .get("delta")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                Some(sse_json_seq(
                    event.seq,
                    "thinking.delta",
                    json!({ "content": content }),
                ))
            } else if kind == "tool_call" {
                let output = payload
                    .get("delta")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                Some(sse_json_seq(
                    event.seq,
                    "tool.progress",
                    json!({ "output": output }),
                ))
            } else {
                None
            }
        }
        "item.started" => {
            let tool = payload.get("tool")?;
            let id = tool.get("id").cloned().unwrap_or(Value::Null);
            let name = tool.get("name").cloned().unwrap_or(Value::Null);
            let input = tool.get("input").cloned().unwrap_or(Value::Null);
            Some(sse_json_seq(
                event.seq,
                "tool.started",
                json!({
                    "id": id,
                    "name": name,
                    "input": input,
                }),
            ))
        }
        "item.completed" | "item.failed" => {
            let item = payload.get("item")?;
            let kind = item
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if kind == "tool_call" || kind == "file_change" || kind == "command_execution" {
                let id = payload
                    .get("tool")
                    .and_then(|t| t.get("id"))
                    .cloned()
                    .unwrap_or_else(|| item.get("id").cloned().unwrap_or(Value::Null));
                let success = event.event == "item.completed";
                let output = item.get("detail").cloned().unwrap_or_else(|| {
                    Value::String(
                        item.get("summary")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    )
                });
                Some(sse_json_seq(
                    event.seq,
                    "tool.completed",
                    json!({
                        "id": id,
                        "success": success,
                        "output": output,
                    }),
                ))
            } else if kind == "status" {
                let message = item
                    .get("detail")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.get("summary").and_then(|v| v.as_str()))
                    .unwrap_or_default();
                Some(sse_json_seq(
                    event.seq,
                    "status",
                    json!({ "message": message }),
                ))
            } else if kind == "error" {
                let message = item
                    .get("detail")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.get("summary").and_then(|v| v.as_str()))
                    .unwrap_or_default();
                Some(sse_json_seq(
                    event.seq,
                    "error",
                    json!({ "message": message }),
                ))
            } else {
                None
            }
        }
        "approval.required" => Some(sse_json_seq(
            event.seq,
            "approval.required",
            payload.clone(),
        )),
        "sandbox.denied" => Some(sse_json_seq(event.seq, "sandbox.denied", payload.clone())),
        "turn.completed" => {
            let usage = payload
                .get("turn")
                .and_then(|turn| turn.get("usage"))
                .cloned()
                .unwrap_or(json!(null));
            Some(sse_json_seq(
                event.seq,
                "turn.completed",
                json!({ "usage": usage }),
            ))
        }
        "agent.spawned" => {
            let agent_id = payload
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if agent_id.is_empty() {
                return None;
            }
            Some(sse_json_seq(
                event.seq,
                "agent.spawned",
                json!({ "agent_id": agent_id }),
            ))
        }
        "agent.progress" => {
            let agent_id = payload
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if agent_id.is_empty() {
                return None;
            }
            let status = payload
                .get("item")
                .and_then(|item| item.get("detail").and_then(|v| v.as_str()))
                .or_else(|| payload.get("status").and_then(|v| v.as_str()))
                .unwrap_or_default();
            Some(sse_json_seq(
                event.seq,
                "agent.progress",
                json!({ "agent_id": agent_id, "status": status }),
            ))
        }
        "agent.completed" => {
            let agent_id = payload
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if agent_id.is_empty() {
                return None;
            }
            let result = payload
                .get("item")
                .and_then(|item| item.get("detail").and_then(|v| v.as_str()))
                .unwrap_or_default();
            Some(sse_json_seq(
                event.seq,
                "agent.completed",
                json!({ "agent_id": agent_id, "result": result }),
            ))
        }
        "agent.list" => {
            let agents = payload.get("agents").cloned().unwrap_or(json!([]));
            Some(sse_json_seq(
                event.seq,
                "agent.list",
                json!({ "agents": agents }),
            ))
        }
        _ => None,
    }
}

fn sse_json(event: &str, payload: serde_json::Value) -> SseEvent {
    let data = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    SseEvent::default().event(event).data(data)
}

/// SSE frame with `id:` set to the durable monotonic `seq` (matches JSON `payload.seq` where present).
///
/// Clients MAY use browser `Last-Event-ID` on reconnect; the authoritative cursor is still
/// `since_seq` on `GET /v1/threads/{id}/events` (this handler does not read `Last-Event-ID` today).
fn sse_json_seq(seq: u64, event: &str, payload: serde_json::Value) -> SseEvent {
    let data = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    SseEvent::default()
        .event(event)
        .id(seq.to_string())
        .data(data)
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{truncated}...")
}

fn collect_workspace_status(workspace: &std::path::Path) -> WorkspaceStatusResponse {
    let mut status = WorkspaceStatusResponse {
        workspace: workspace.to_path_buf(),
        git_repo: false,
        branch: None,
        staged: 0,
        unstaged: 0,
        untracked: 0,
        ahead: None,
        behind: None,
    };

    let Some(repo_check) = run_git(workspace, &["rev-parse", "--is-inside-work-tree"]) else {
        return status;
    };
    if repo_check.trim() != "true" {
        return status;
    }

    status.git_repo = true;
    status.branch = run_git(workspace, &["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(porcelain) = run_git(workspace, &["status", "--porcelain=v1"]) {
        for line in porcelain.lines() {
            if line.starts_with("??") {
                status.untracked += 1;
                continue;
            }
            let chars: Vec<char> = line.chars().collect();
            if chars.len() >= 2 {
                if chars[0] != ' ' {
                    status.staged += 1;
                }
                if chars[1] != ' ' {
                    status.unstaged += 1;
                }
            }
        }
    }

    if let Some(counts) = run_git(
        workspace,
        &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"],
    ) {
        let mut parts = counts.split_whitespace();
        if let (Some(behind), Some(ahead)) = (parts.next(), parts.next()) {
            status.behind = behind.parse::<u32>().ok();
            status.ahead = ahead.parse::<u32>().ok();
        }
    }

    status
}

fn run_git(workspace: &std::path::Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn validate_skill_directory_name(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }
    if s.len() > 96 {
        return Err(ApiError::bad_request("name is too long (max 96)"));
    }
    if s == "." || s == ".." {
        return Err(ApiError::bad_request("invalid skill name"));
    }
    for c in s.chars() {
        if c == '/' || c == '\\' {
            return Err(ApiError::bad_request(
                "skill name must not contain path separators",
            ));
        }
        if !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') {
            return Err(ApiError::bad_request(
                "skill name may only contain ASCII letters, digits, '.', '-', and '_'",
            ));
        }
    }
    Ok(s.to_string())
}

fn skill_md_template(name: &str) -> String {
    format!(
        r#"---
name: {name}
description: Describe what this skill does.
allowed-tools: read_file, list_dir
---

Write skill instructions here.
"#
    )
}

/// Skills roots that exist (and global dir, which is created if missing) for
/// validating `parent_directory` from the desktop folder picker.
fn allowed_skill_roots_for_picker(
    config: &Config,
    workspace: &std::path::Path,
) -> Result<Vec<PathBuf>, ApiError> {
    let mut roots: Vec<PathBuf> = Vec::new();

    let global = config.skills_dir();
    fs::create_dir_all(&global).map_err(|e| {
        ApiError::internal(format!(
            "failed to ensure global skills dir {}: {e}",
            global.display()
        ))
    })?;
    let global_canon = global.canonicalize().map_err(|e| {
        ApiError::internal(format!("failed to canonicalize global skills dir: {e}"))
    })?;
    roots.push(global_canon);

    let ws = workspace
        .canonicalize()
        .map_err(|e| ApiError::bad_request(format!("workspace path could not be resolved: {e}")))?;
    for rel in [".agents/skills", "skills"] {
        let p = ws.join(rel);
        if p.is_dir() {
            if let Ok(c) = p.canonicalize() {
                roots.push(c);
            }
        }
    }

    roots.sort_unstable();
    roots.dedup();
    Ok(roots)
}

fn resolve_create_skill_parent(
    config: &Config,
    workspace: &std::path::Path,
    parent_directory: Option<&PathBuf>,
    scope: &str,
) -> Result<PathBuf, ApiError> {
    if let Some(user_parent) = parent_directory {
        let user = user_parent.canonicalize().map_err(|_| {
            ApiError::bad_request(
                "parent_directory must exist and be readable (pick an existing skills root)",
            )
        })?;
        let allowed = allowed_skill_roots_for_picker(config, workspace)?;
        if !allowed.iter().any(|r| r == &user) {
            return Err(ApiError::bad_request(
                "parent_directory is not an allowed skills root; use global or workspace skills directory",
            ));
        }
        return Ok(user);
    }

    match scope.trim().to_ascii_lowercase().as_str() {
        "global" => {
            let global = config.skills_dir();
            fs::create_dir_all(&global).map_err(|e| {
                ApiError::internal(format!(
                    "failed to create global skills dir {}: {e}",
                    global.display()
                ))
            })?;
            Ok(global)
        }
        "workspace" => {
            let ws = workspace.canonicalize().map_err(|e| {
                ApiError::bad_request(format!("workspace path could not be resolved: {e}"))
            })?;
            let agents = ws.join(".agents").join("skills");
            let flat = ws.join("skills");
            if agents.is_dir() {
                Ok(agents)
            } else if flat.is_dir() {
                Ok(flat)
            } else {
                fs::create_dir_all(&agents).map_err(|e| {
                    ApiError::internal(format!(
                        "failed to create workspace skills dir {}: {e}",
                        agents.display()
                    ))
                })?;
                Ok(agents)
            }
        }
        _ => Err(ApiError::bad_request(
            "scope must be \"global\" or \"workspace\" (or pass parent_directory)",
        )),
    }
}

fn resolve_skills_dir(config: &Config, workspace: &std::path::Path) -> PathBuf {
    let agents_skills = workspace.join(".agents").join("skills");
    if agents_skills.exists() {
        return agents_skills;
    }
    let local_skills = workspace.join("skills");
    if local_skills.exists() {
        return local_skills;
    }
    config.skills_dir()
}

fn load_mcp_config_or_default(path: &std::path::Path) -> Result<McpConfig, ApiError> {
    if !path.exists() {
        return Ok(McpConfig::default());
    }
    let raw = fs::read_to_string(path).map_err(|e| {
        ApiError::internal(format!("Failed to read MCP config {}: {e}", path.display()))
    })?;
    serde_json::from_str::<McpConfig>(&raw).map_err(|e| {
        ApiError::internal(format!(
            "Failed to parse MCP config {}: {e}",
            path.display()
        ))
    })
}

#[derive(Debug, Deserialize)]
struct UsageQuery {
    /// ISO-8601 lower bound (inclusive). When omitted, no lower bound.
    since: Option<String>,
    /// ISO-8601 upper bound (inclusive). When omitted, no upper bound.
    until: Option<String>,
    /// Bucket key. One of `day` (default), `model`, `provider`, `thread`.
    group_by: Option<String>,
}

fn parse_iso8601(raw: &str, field: &str) -> Result<chrono::DateTime<Utc>, ApiError> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| ApiError::bad_request(format!("Invalid {field} (expected RFC 3339): {e}")))
}

async fn get_usage(
    State(state): State<RuntimeApiState>,
    Query(query): Query<UsageQuery>,
) -> Result<Json<Value>, ApiError> {
    let since = match query.since.as_deref() {
        Some(raw) => Some(parse_iso8601(raw, "since")?),
        None => None,
    };
    let until = match query.until.as_deref() {
        Some(raw) => Some(parse_iso8601(raw, "until")?),
        None => None,
    };
    if let (Some(s), Some(u)) = (since, until)
        && s > u
    {
        return Err(ApiError::bad_request("since must be <= until".to_string()));
    }
    let group_by = match query.group_by.as_deref().unwrap_or("day") {
        "day" => UsageGroupBy::Day,
        "model" => UsageGroupBy::Model,
        "provider" => UsageGroupBy::Provider,
        "thread" => UsageGroupBy::Thread,
        other => {
            return Err(ApiError::bad_request(format!(
                "Unsupported group_by '{other}': expected one of day, model, provider, thread"
            )));
        }
    };

    let aggregation = state
        .runtime_threads
        .aggregate_usage(since, until, group_by)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(json!(aggregation)))
}

// ============== Routing Rules ==============

async fn get_routing_rules(State(state): State<RuntimeApiState>) -> Result<Json<Value>, ApiError> {
    let rules = state.runtime_threads.get_routing_rules().await;
    Ok(Json(json!({ "rules": rules })))
}

async fn set_routing_rules(
    State(state): State<RuntimeApiState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Value>, ApiError> {
    let rules: Vec<crate::runtime_threads::RoutingRule> = serde_json::from_value(
        body.get("rules")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![])),
    )
    .map_err(|e| ApiError::bad_request(format!("Invalid rules: {e}")))?;
    state
        .runtime_threads
        .set_routing_rules(rules)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let updated = state.runtime_threads.get_routing_rules().await;
    Ok(Json(json!({ "rules": updated })))
}

/// Built-in dev origins always allowed by the runtime API (whalescale#255).

// ============== Symbol Index Rebuild ==============

/// Query params for symbol index rebuild.
#[derive(Deserialize)]
struct RebuildSymbolIndexQuery {
    workspace: String,
}

async fn rebuild_symbol_index(
    State(_state): State<RuntimeApiState>,
    Query(q): Query<RebuildSymbolIndexQuery>,
) -> Result<Json<Value>, ApiError> {
    let ws = PathBuf::from(&q.workspace);
    if !ws.is_dir() {
        return Err(ApiError::bad_request(format!(
            "Not a directory: {}",
            q.workspace
        )));
    }
    let ws_for_build = ws.clone();
    let path = ws.join(".deepseek").join("symbols.json");
    let index = tokio::task::spawn_blocking(move || {
        crate::symbol_index::build_index(&ws_for_build, crate::symbol_index::SymbolVisibility::Public)
    })
    .await
    .map_err(|e| ApiError::internal(format!("build_index panicked: {e}")))?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(&index).unwrap_or_default();
    std::fs::write(&path, &json).map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(json!({
        "status": "ok",
        "path": path.to_string_lossy(),
        "symbol_count": index.files.len(),
    })))
}

/// Built-in dev origins always allowed by the runtime API (whalescale#255).
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

fn cors_layer(extra_origins: &[String]) -> CorsLayer {
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

fn map_task_err(err: anyhow::Error) -> ApiError {
    let message = err.to_string();
    if message.contains("not found") {
        ApiError::not_found(message)
    } else {
        ApiError::bad_request(message)
    }
}

fn map_automation_err(err: anyhow::Error) -> ApiError {
    let message = err.to_string();
    if message.contains("Failed to read automation")
        || message.contains("No such file or directory")
    {
        ApiError::not_found(message)
    } else {
        ApiError::bad_request(message)
    }
}

fn map_thread_err(err: anyhow::Error) -> ApiError {
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
struct ApiError {
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
        (
            self.status,
            Json(json!({
                "error": {
                    "message": self.message,
                    "status": self.status.as_u16(),
                }
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::events::{Event as EngineEvent, TurnOutcomeStatus};
    use crate::core::ops::Op;
    use crate::models::Usage;
    use crate::runtime_threads::RuntimeEventRecord;
    use anyhow::{Context, bail};
    use futures_util::StreamExt;
    use std::fs;
    use std::sync::Arc;
    use tokio::sync::{Mutex, mpsc};
    use tokio::time::sleep;
    use uuid::Uuid;

    #[test]
    fn skill_directory_name_validation() {
        assert_eq!(
            validate_skill_directory_name("my-skill").unwrap(),
            "my-skill"
        );
        assert_eq!(
            validate_skill_directory_name("  trim_me  ").unwrap(),
            "trim_me"
        );
        assert!(validate_skill_directory_name("").is_err());
        assert!(validate_skill_directory_name("a/b").is_err());
        assert!(validate_skill_directory_name(".").is_err());
        assert!(validate_skill_directory_name("..").is_err());
        assert!(validate_skill_directory_name("bad name").is_err());
    }

    struct MockExecutor;

    #[async_trait::async_trait]
    impl crate::task_manager::TaskExecutor for MockExecutor {
        async fn execute(
            &self,
            _task: crate::task_manager::ExecutionTask,
            events: mpsc::UnboundedSender<crate::task_manager::TaskExecutionEvent>,
            cancel: tokio_util::sync::CancellationToken,
        ) -> crate::task_manager::TaskExecutionResult {
            let _ = events.send(crate::task_manager::TaskExecutionEvent::Status {
                message: "started".to_string(),
            });
            sleep(Duration::from_millis(100)).await;
            if cancel.is_cancelled() {
                return crate::task_manager::TaskExecutionResult {
                    status: crate::task_manager::TaskStatus::Canceled,
                    result_text: None,
                    error: None,
                };
            }
            crate::task_manager::TaskExecutionResult {
                status: crate::task_manager::TaskStatus::Completed,
                result_text: Some("ok".to_string()),
                error: None,
            }
        }
    }

    async fn spawn_test_server_with_root(
        root: PathBuf,
        sessions_dir: PathBuf,
    ) -> Result<
        Option<(
            SocketAddr,
            SharedRuntimeThreadManager,
            tokio::task::JoinHandle<()>,
        )>,
    > {
        spawn_test_server_with_root_and_token(root, sessions_dir, None).await
    }

    async fn spawn_test_server_with_root_and_token(
        root: PathBuf,
        sessions_dir: PathBuf,
        runtime_token: Option<String>,
    ) -> Result<
        Option<(
            SocketAddr,
            SharedRuntimeThreadManager,
            tokio::task::JoinHandle<()>,
        )>,
    > {
        fs::create_dir_all(&sessions_dir)?;
        let manager = TaskManager::start_with_executor(
            TaskManagerConfig {
                data_dir: root.join("tasks"),
                worker_count: 1,
                default_workspace: PathBuf::from("."),
                default_model: DEFAULT_TEXT_MODEL.to_string(),
                default_mode: "agent".to_string(),
                allow_shell: false,
                trust_mode: false,
                max_subagents: 2,
            },
            Arc::new(MockExecutor),
        )
        .await?;
        let mut config = Config::default();
        config.capacity = Some(crate::config::CapacityConfig {
            enabled: Some(false),
            low_risk_max: None,
            medium_risk_max: None,
            severe_min_slack: None,
            severe_violation_ratio: None,
            refresh_cooldown_turns: None,
            replan_cooldown_turns: None,
            max_replay_per_turn: None,
            min_turns_before_guardrail: None,
            profile_window: None,
            deepseek_v3_2_chat_prior: None,
            deepseek_v3_2_reasoner_prior: None,
            deepseek_v4_pro_prior: None,
            deepseek_v4_flash_prior: None,
            fallback_default_prior: None,
        });
        let runtime_threads: SharedRuntimeThreadManager = Arc::new(RuntimeThreadManager::open(
            config,
            PathBuf::from("."),
            RuntimeThreadManagerConfig::from_task_data_dir(root.join("runtime")),
        )?);
        runtime_threads.attach_task_manager(manager.clone());
        let automations = Arc::new(Mutex::new(AutomationManager::open(
            root.join("automations"),
        )?));
        runtime_threads.attach_automation_manager(automations.clone());

        let token_fp = {
            let mut hasher = Sha256::new();
            hasher.update(runtime_token.as_deref().unwrap_or(""));
            let hash = hasher.finalize();
            let fp: String = hash[..16].iter().map(|b| format!("{b:02x}")).collect();
            fp
        };
        let shared_sm = Arc::new(SessionManager::new(sessions_dir.clone())?);

        let state = RuntimeApiState {
            config: Config::default(),
            workspace: PathBuf::from("."),
            task_manager: manager,
            runtime_threads: runtime_threads.clone(),
            cors_origins: Vec::new(),
            mcp_config_path: root.join("mcp.json"),
            automations,
            runtime_token,
            process_started_at_ms: 0,
            token_fingerprint: Arc::new(token_fp),
            shared_session_manager: shared_sm,
            resume_tracker: ResumeTaskTracker::new(),
        };
        let app = build_router(state);
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        let addr = listener.local_addr()?;
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Some((addr, runtime_threads, handle)))
    }

    async fn spawn_test_server() -> Result<
        Option<(
            SocketAddr,
            SharedRuntimeThreadManager,
            tokio::task::JoinHandle<()>,
        )>,
    > {
        let root = std::env::temp_dir().join(format!("deepseek-runtime-api-{}", Uuid::new_v4()));
        let sessions_dir = root.join("sessions");
        spawn_test_server_with_root(root, sessions_dir).await
    }

    async fn read_first_sse_frame(resp: reqwest::Response) -> Result<String> {
        let mut stream = resp.bytes_stream();
        let mut buf = Vec::new();
        loop {
            let next = tokio::time::timeout(Duration::from_secs(2), stream.next())
                .await
                .context("timed out waiting for SSE frame")?
                .context("SSE stream ended unexpectedly")??;
            buf.extend_from_slice(&next);

            let text = String::from_utf8_lossy(&buf);
            if let Some(idx) = text.find("\n\n").or_else(|| text.find("\r\n\r\n")) {
                return Ok(text[..idx].to_string());
            }

            if buf.len() > 64 * 1024 {
                bail!("SSE frame exceeded 64KB without delimiter");
            }
        }
    }

    fn parse_sse_frame(frame: &str) -> Result<(String, serde_json::Value)> {
        let mut event_name: Option<String> = None;
        let mut data_lines = Vec::new();
        for line in frame.lines() {
            if let Some(rest) = line.strip_prefix("event:") {
                event_name = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim_start().to_string());
            }
        }
        let event_name = event_name.context("missing SSE event field")?;
        let payload = if data_lines.is_empty() {
            json!({})
        } else {
            serde_json::from_str(&data_lines.join("\n"))
                .with_context(|| format!("invalid SSE data payload: {}", data_lines.join("\n")))?
        };
        Ok((event_name, payload))
    }

    async fn wait_for_terminal_turn_status(
        client: &reqwest::Client,
        addr: SocketAddr,
        thread_id: &str,
        turn_id: &str,
        timeout: Duration,
    ) -> Result<String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let detail: serde_json::Value = client
                .get(format!("http://{addr}/v1/threads/{thread_id}"))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let status = detail["turns"]
                .as_array()
                .and_then(|turns| turns.iter().find(|turn| turn["id"] == turn_id))
                .and_then(|turn| turn.get("status"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if matches!(
                status.as_str(),
                "completed" | "failed" | "interrupted" | "canceled"
            ) {
                return Ok(status);
            }
            if tokio::time::Instant::now() >= deadline {
                bail!("timed out waiting for terminal turn status for {turn_id}");
            }
            sleep(Duration::from_millis(25)).await;
        }
    }

    #[tokio::test]
    async fn health_and_tasks_endpoints_work() -> Result<()> {
        let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let health: serde_json::Value = client
            .get(format!("http://{addr}/health"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(health["status"], "ok");

        let created: serde_json::Value = client
            .post(format!("http://{addr}/v1/tasks"))
            .json(&json!({ "prompt": "hello task" }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let id = created["id"].as_str().expect("task id").to_string();

        let listed: serde_json::Value = client
            .get(format!("http://{addr}/v1/tasks"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert!(
            listed["tasks"]
                .as_array()
                .is_some_and(|tasks| !tasks.is_empty())
        );

        let detail: serde_json::Value = client
            .get(format!("http://{addr}/v1/tasks/{id}"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(detail["id"], id);

        let _cancelled: serde_json::Value = client
            .post(format!("http://{addr}/v1/tasks/{id}/cancel"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn runtime_token_guard_protects_v1_routes() -> Result<()> {
        let root = std::env::temp_dir().join(format!("deepseek-runtime-api-{}", Uuid::new_v4()));
        let sessions_dir = root.join("sessions");
        let token = "local-test-token".to_string();
        let Some((addr, _runtime_threads, handle)) =
            spawn_test_server_with_root_and_token(root, sessions_dir, Some(token.clone())).await?
        else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let health = client
            .get(format!("http://{addr}/health"))
            .send()
            .await?
            .error_for_status()?;
        assert_eq!(health.status(), StatusCode::OK);

        let unauthorized = client
            .get(format!("http://{addr}/v1/threads/summary"))
            .send()
            .await?;
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let bearer = client
            .get(format!("http://{addr}/v1/threads/summary"))
            .bearer_auth(&token)
            .send()
            .await?
            .error_for_status()?;
        assert_eq!(bearer.status(), StatusCode::OK);

        // Query-string tokens are intentionally not accepted (log/Referer leakage).
        let query_only = client
            .get(format!("http://{addr}/v1/threads/summary?token={token}"))
            .send()
            .await?;
        assert_eq!(query_only.status(), StatusCode::UNAUTHORIZED);

        let header_token = client
            .get(format!("http://{addr}/v1/threads/summary"))
            .header("x-deepseek-runtime-token", &token)
            .send()
            .await?
            .error_for_status()?;
        assert_eq!(header_token.status(), StatusCode::OK);

        handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn workspace_and_automation_endpoints_work() -> Result<()> {
        let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let workspace: serde_json::Value = client
            .get(format!("http://{addr}/v1/workspace/status"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert!(workspace.get("workspace").is_some());

        let created: serde_json::Value = client
            .post(format!("http://{addr}/v1/automations"))
            .json(&json!({
                "name": "Smoke automation",
                "prompt": "automation smoke test",
                "rrule": "FREQ=HOURLY;INTERVAL=2",
                "status": "active"
            }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let automation_id = created["id"]
            .as_str()
            .context("missing automation id")?
            .to_string();

        let listed: serde_json::Value = client
            .get(format!("http://{addr}/v1/automations"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert!(
            listed
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["id"] == automation_id))
        );

        let run_now: serde_json::Value = client
            .post(format!("http://{addr}/v1/automations/{automation_id}/run"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(run_now["automation_id"], automation_id);

        let paused: serde_json::Value = client
            .post(format!(
                "http://{addr}/v1/automations/{automation_id}/pause"
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(paused["status"], "paused");

        let resumed: serde_json::Value = client
            .post(format!(
                "http://{addr}/v1/automations/{automation_id}/resume"
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(resumed["status"], "active");

        let updated: serde_json::Value = client
            .patch(format!("http://{addr}/v1/automations/{automation_id}"))
            .json(&json!({
                "name": "Smoke automation edited",
                "rrule": "FREQ=WEEKLY;BYDAY=MO,WE;BYHOUR=10;BYMINUTE=15"
            }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(updated["name"], "Smoke automation edited");

        let runs: serde_json::Value = client
            .get(format!(
                "http://{addr}/v1/automations/{automation_id}/runs?limit=5"
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert!(
            runs.as_array().is_some_and(|items| !items.is_empty()),
            "expected at least one run entry"
        );

        let _deleted: serde_json::Value = client
            .delete(format!("http://{addr}/v1/automations/{automation_id}"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let missing_status = client
            .get(format!("http://{addr}/v1/automations/{automation_id}"))
            .send()
            .await?
            .status();
        assert_eq!(missing_status, StatusCode::NOT_FOUND);

        handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn stream_requires_prompt() -> Result<()> {
        let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("http://{addr}/v1/stream"))
            .json(&json!({ "prompt": "" }))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn thread_endpoints_expose_lifecycle_contract() -> Result<()> {
        let Some((addr, runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let created: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads"))
            .json(&json!({}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let thread_id = created["id"]
            .as_str()
            .context("missing thread id")?
            .to_string();

        let archived: serde_json::Value = client
            .patch(format!("http://{addr}/v1/threads/{thread_id}"))
            .json(&json!({ "archived": true }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(archived["id"], thread_id);
        assert_eq!(archived["archived"], true);

        let listed: serde_json::Value = client
            .get(format!("http://{addr}/v1/threads"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert!(
            listed
                .as_array()
                .is_some_and(|threads| threads.iter().all(|t| t["id"] != thread_id))
        );

        let listed_all: serde_json::Value = client
            .get(format!(
                "http://{addr}/v1/threads/summary?include_archived=true&limit=100"
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert!(
            listed_all
                .as_array()
                .is_some_and(|threads| threads.iter().any(|t| t["id"] == thread_id))
        );

        let unarchived: serde_json::Value = client
            .patch(format!("http://{addr}/v1/threads/{thread_id}"))
            .json(&json!({ "archived": false }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(unarchived["archived"], false);

        let invalid_patch = client
            .patch(format!("http://{addr}/v1/threads/{thread_id}"))
            .json(&json!({}))
            .send()
            .await?;
        assert_eq!(invalid_patch.status(), StatusCode::BAD_REQUEST);

        let missing_patch = client
            .patch(format!("http://{addr}/v1/threads/thr_missing"))
            .json(&json!({ "archived": true }))
            .send()
            .await?;
        assert_eq!(missing_patch.status(), StatusCode::NOT_FOUND);

        let detail: serde_json::Value = client
            .get(format!("http://{addr}/v1/threads/{thread_id}"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(detail["thread"]["id"], thread_id);

        let resumed: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads/{thread_id}/resume"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(resumed["id"], thread_id);

        let forked: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads/{thread_id}/fork"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let forked_id = forked["id"].as_str().context("missing forked id")?;
        assert_ne!(forked_id, thread_id);

        // Install a mock engine so the turn completes without calling the real API.
        // The mock handles both SendMessage and CompactContext ops so the
        // compact endpoint tested later also works.
        let harness = crate::core::engine::mock_engine_handle();
        runtime_threads
            .install_test_engine(&thread_id, harness.handle.clone())
            .await?;
        let mut rx_op = harness.rx_op;
        let tx_event = harness.tx_event;
        tokio::spawn(async move {
            while let Some(op) = rx_op.recv().await {
                match op {
                    Op::SendMessage { .. } => {
                        let _ = tx_event
                            .send(EngineEvent::TurnStarted {
                                turn_id: "mock_lifecycle".to_string(),
                            })
                            .await;
                        let _ = tx_event
                            .send(EngineEvent::MessageStarted { index: 0 })
                            .await;
                        let _ = tx_event
                            .send(EngineEvent::MessageDelta {
                                index: 0,
                                content: "mock reply".to_string(),
                            })
                            .await;
                        let _ = tx_event
                            .send(EngineEvent::MessageComplete { index: 0 })
                            .await;
                        let _ = tx_event
                            .send(EngineEvent::TurnComplete {
                                usage: Usage {
                                    input_tokens: 10,
                                    output_tokens: 5,
                                    ..Usage::default()
                                },
                                last_request_input_tokens: None,
                                status: TurnOutcomeStatus::Completed,
                                error: None,
                            })
                            .await;
                    }
                    Op::CompactContext => {
                        let _ = tx_event
                            .send(EngineEvent::TurnComplete {
                                usage: Usage {
                                    input_tokens: 0,
                                    output_tokens: 0,
                                    ..Usage::default()
                                },
                                last_request_input_tokens: None,
                                status: TurnOutcomeStatus::Completed,
                                error: None,
                            })
                            .await;
                    }
                    _ => {}
                }
            }
        });

        let turn_start: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads/{thread_id}/turns"))
            .json(&json!({ "prompt": "thread endpoint test" }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let turn_id = turn_start["turn"]["id"]
            .as_str()
            .context("missing turn id")?
            .to_string();

        let _ = wait_for_terminal_turn_status(
            &client,
            addr,
            &thread_id,
            &turn_id,
            Duration::from_secs(2),
        )
        .await?;

        {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
            while runtime_threads
                .active_turn_flags(&thread_id, &turn_id)
                .await
                .is_some()
            {
                if tokio::time::Instant::now() >= deadline {
                    bail!("timed out waiting for active_turn to clear on thread {thread_id}");
                }
                sleep(Duration::from_millis(25)).await;
            }
        }

        let steer_resp = client
            .post(format!(
                "http://{addr}/v1/threads/{thread_id}/turns/{turn_id}/steer"
            ))
            .json(&json!({ "prompt": "late steer" }))
            .send()
            .await?;
        assert_eq!(steer_resp.status(), StatusCode::CONFLICT);

        let interrupt_resp = client
            .post(format!(
                "http://{addr}/v1/threads/{thread_id}/turns/{turn_id}/interrupt"
            ))
            .send()
            .await?;
        assert_eq!(interrupt_resp.status(), StatusCode::CONFLICT);

        let compact_start: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads/{thread_id}/compact"))
            .json(&json!({ "reason": "test manual compact" }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(compact_start["thread"]["id"], thread_id);

        let events_resp = client
            .get(format!(
                "http://{addr}/v1/threads/{thread_id}/events?since_seq=0"
            ))
            .send()
            .await?
            .error_for_status()?;
        let content_type = events_resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.starts_with("text/event-stream"));
        let chunk_text = read_first_sse_frame(events_resp).await?;
        assert!(
            chunk_text.contains("event:"),
            "expected SSE event chunk, got: {chunk_text}"
        );

        handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn events_endpoint_accepts_replay_only_one_query() -> Result<()> {
        let Some((addr, runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let created: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads"))
            .json(&json!({}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let thread_id = created["id"]
            .as_str()
            .context("thread id missing")?
            .to_string();

        let events_resp = client
            .get(format!(
                "http://{addr}/v1/threads/{thread_id}/events?since_seq=0&replay_only=1"
            ))
            .send()
            .await?;
        assert_eq!(events_resp.status(), StatusCode::OK);
        let content_type = events_resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(content_type.starts_with("text/event-stream"));

        handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn events_endpoint_respects_since_seq_cursor() -> Result<()> {
        let Some((addr, runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let created: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads"))
            .json(&json!({}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let thread_id = created["id"]
            .as_str()
            .context("missing thread id")?
            .to_string();

        // Install a mock engine so the turn completes without calling the real API.
        let harness = crate::core::engine::mock_engine_handle();
        runtime_threads
            .install_test_engine(&thread_id, harness.handle.clone())
            .await?;
        let mut rx_op = harness.rx_op;
        let tx_event = harness.tx_event;
        tokio::spawn(async move {
            if !matches!(rx_op.recv().await, Some(Op::SendMessage { .. })) {
                return;
            }
            let _ = tx_event
                .send(EngineEvent::TurnStarted {
                    turn_id: "mock_cursor".to_string(),
                })
                .await;
            let _ = tx_event
                .send(EngineEvent::MessageStarted { index: 0 })
                .await;
            let _ = tx_event
                .send(EngineEvent::MessageComplete { index: 0 })
                .await;
            let _ = tx_event
                .send(EngineEvent::TurnComplete {
                    usage: Usage {
                        input_tokens: 5,
                        output_tokens: 3,
                        ..Usage::default()
                    },
                    last_request_input_tokens: None,
                    status: TurnOutcomeStatus::Completed,
                    error: None,
                })
                .await;
        });

        let started: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads/{thread_id}/turns"))
            .json(&json!({ "prompt": "cursor replay test" }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let turn_id = started["turn"]["id"]
            .as_str()
            .context("missing turn id")?
            .to_string();

        let _ = wait_for_terminal_turn_status(
            &client,
            addr,
            &thread_id,
            &turn_id,
            Duration::from_secs(2),
        )
        .await?;

        let resp_a = client
            .get(format!(
                "http://{addr}/v1/threads/{thread_id}/events?since_seq=0"
            ))
            .send()
            .await?
            .error_for_status()?;
        let frame_a = read_first_sse_frame(resp_a).await?;
        let (_event_a, payload_a) = parse_sse_frame(&frame_a)?;
        let seq_a = payload_a
            .get("seq")
            .and_then(Value::as_u64)
            .context("missing seq in first replay frame")?;
        assert!(
            frame_a.contains("id:") && frame_a.contains(&seq_a.to_string()),
            "expected SSE id field to match seq, frame: {frame_a}"
        );

        let resp_b = client
            .get(format!(
                "http://{addr}/v1/threads/{thread_id}/events?since_seq={seq_a}"
            ))
            .send()
            .await?
            .error_for_status()?;
        let frame_b = read_first_sse_frame(resp_b).await?;
        let (_event_b, payload_b) = parse_sse_frame(&frame_b)?;
        let seq_b = payload_b
            .get("seq")
            .and_then(Value::as_u64)
            .context("missing seq in second replay frame")?;
        assert!(
            seq_b > seq_a,
            "expected seq after cursor: {seq_b} <= {seq_a}"
        );
        assert_eq!(payload_b["thread_id"], thread_id);

        handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn steer_and_interrupt_endpoints_work_on_active_turn() -> Result<()> {
        let Some((addr, runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let created: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads"))
            .json(&json!({}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let thread_id = created["id"]
            .as_str()
            .context("missing thread id")?
            .to_string();

        let harness = crate::core::engine::mock_engine_handle();
        runtime_threads
            .install_test_engine(&thread_id, harness.handle.clone())
            .await?;
        let mut rx_op = harness.rx_op;
        let mut rx_steer = harness.rx_steer;
        let tx_event = harness.tx_event;
        let cancel_token = harness.cancel_token;
        tokio::spawn(async move {
            if !matches!(rx_op.recv().await, Some(Op::SendMessage { .. })) {
                return;
            }
            let _ = tx_event
                .send(EngineEvent::TurnStarted {
                    turn_id: "engine_turn_api".to_string(),
                })
                .await;
            let _ = tx_event
                .send(EngineEvent::MessageStarted { index: 0 })
                .await;
            if let Some(steer_text) = rx_steer.recv().await {
                let _ = tx_event
                    .send(EngineEvent::MessageDelta {
                        index: 0,
                        content: format!("steer:{steer_text}"),
                    })
                    .await;
            }
            cancel_token.cancelled().await;
            sleep(Duration::from_millis(60)).await;
            let _ = tx_event
                .send(EngineEvent::TurnComplete {
                    usage: Usage {
                        input_tokens: 2,
                        output_tokens: 1,
                        ..Usage::default()
                    },
                    last_request_input_tokens: None,
                    status: TurnOutcomeStatus::Completed,
                    error: None,
                })
                .await;
        });

        let turn_start: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads/{thread_id}/turns"))
            .json(&json!({ "prompt": "active controls" }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let turn_id = turn_start["turn"]["id"]
            .as_str()
            .context("missing turn id")?
            .to_string();

        let steer_resp: serde_json::Value = client
            .post(format!(
                "http://{addr}/v1/threads/{thread_id}/turns/{turn_id}/steer"
            ))
            .json(&json!({ "prompt": "please steer" }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(steer_resp["id"], turn_id);
        assert_eq!(steer_resp["steer_count"], 1);

        let interrupt_resp: serde_json::Value = client
            .post(format!(
                "http://{addr}/v1/threads/{thread_id}/turns/{turn_id}/interrupt"
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(interrupt_resp["id"], turn_id);

        let terminal = wait_for_terminal_turn_status(
            &client,
            addr,
            &thread_id,
            &turn_id,
            Duration::from_secs(3),
        )
        .await?;
        assert_eq!(terminal, "interrupted");

        let events = runtime_threads.events_since(&thread_id, None)?;
        assert!(events.iter().any(|ev| ev.event == "turn.steered"));
        assert!(
            events
                .iter()
                .any(|ev| ev.event == "turn.interrupt_requested")
        );
        assert!(events.iter().any(|ev| {
            ev.event == "turn.completed"
                && ev
                    .payload
                    .get("turn")
                    .and_then(|turn| turn.get("status"))
                    .and_then(Value::as_str)
                    == Some("interrupted")
        }));

        handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn stream_compat_mapping_handles_expected_runtime_events() -> Result<()> {
        let agent_delta = RuntimeEventRecord {
            schema_version: 1,
            seq: 1,
            timestamp: chrono::Utc::now(),
            thread_id: "thr_test".to_string(),
            turn_id: Some("turn_test".to_string()),
            item_id: Some("item_test".to_string()),
            event: "item.delta".to_string(),
            payload: json!({
                "kind": "agent_message",
                "delta": "hello",
            }),
        };
        let mapped = map_compat_stream_event(&agent_delta).context("missing mapped SSE event")?;
        let stream = async_stream::stream! {
            yield Ok::<_, Infallible>(mapped);
        };
        let body =
            axum::body::to_bytes(Sse::new(stream).into_response().into_body(), usize::MAX).await?;
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("event: message.delta"));
        assert!(text.contains("\"content\":\"hello\""));

        let think_delta = RuntimeEventRecord {
            schema_version: 1,
            seq: 10,
            timestamp: chrono::Utc::now(),
            thread_id: "thr_test".to_string(),
            turn_id: Some("turn_test".to_string()),
            item_id: Some("item_think".to_string()),
            event: "item.delta".to_string(),
            payload: json!({
                "kind": "thinking",
                "delta": "step 1…",
            }),
        };
        let mapped = map_compat_stream_event(&think_delta).context("missing thinking SSE event")?;
        let stream = async_stream::stream! {
            yield Ok::<_, Infallible>(mapped);
        };
        let body =
            axum::body::to_bytes(Sse::new(stream).into_response().into_body(), usize::MAX).await?;
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("event: thinking.delta"));
        assert!(text.contains("\"content\":\"step 1"));

        let tool_start = RuntimeEventRecord {
            schema_version: 1,
            seq: 2,
            timestamp: chrono::Utc::now(),
            thread_id: "thr_test".to_string(),
            turn_id: Some("turn_test".to_string()),
            item_id: Some("item_tool".to_string()),
            event: "item.started".to_string(),
            payload: json!({
                "tool": { "id": "tool_1", "name": "exec_shell", "input": { "cmd": "pwd" } }
            }),
        };
        let mapped = map_compat_stream_event(&tool_start).context("missing tool.started event")?;
        let stream = async_stream::stream! {
            yield Ok::<_, Infallible>(mapped);
        };
        let body =
            axum::body::to_bytes(Sse::new(stream).into_response().into_body(), usize::MAX).await?;
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("event: tool.started"));

        let tool_done = RuntimeEventRecord {
            schema_version: 1,
            seq: 3,
            timestamp: chrono::Utc::now(),
            thread_id: "thr_test".to_string(),
            turn_id: Some("turn_test".to_string()),
            item_id: Some("item_tool".to_string()),
            event: "item.completed".to_string(),
            payload: json!({
                "item": {
                    "id": "item_tool",
                    "kind": "tool_call",
                    "summary": "ok",
                    "detail": "done"
                },
                "tool": { "id": "tool_1", "name": "exec_shell" }
            }),
        };
        let mapped = map_compat_stream_event(&tool_done).context("missing tool.completed event")?;
        let stream = async_stream::stream! {
            yield Ok::<_, Infallible>(mapped);
        };
        let body =
            axum::body::to_bytes(Sse::new(stream).into_response().into_body(), usize::MAX).await?;
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("event: tool.completed"));
        assert!(text.contains("\"success\":true"));
        assert!(
            text.contains("\"id\":\"tool_1\""),
            "tool.completed must use engine tool id, not item id: {text}"
        );

        let unknown = RuntimeEventRecord {
            schema_version: 1,
            seq: 4,
            timestamp: chrono::Utc::now(),
            thread_id: "thr_test".to_string(),
            turn_id: Some("turn_test".to_string()),
            item_id: None,
            event: "item.delta".to_string(),
            payload: json!({
                "kind": "context_compaction",
                "delta": "ignored",
            }),
        };
        assert!(map_compat_stream_event(&unknown).is_none());
        Ok(())
    }

    #[tokio::test]
    async fn stream_endpoint_remains_backward_compatible() -> Result<()> {
        let Some((addr, runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        // Create a thread and install a mock engine so /v1/stream doesn't call the real API.
        let created: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads"))
            .json(&json!({}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let thread_id = created["id"]
            .as_str()
            .context("missing thread id")?
            .to_string();

        let harness = crate::core::engine::mock_engine_handle();
        runtime_threads
            .install_test_engine(&thread_id, harness.handle.clone())
            .await?;
        let mut rx_op = harness.rx_op;
        let tx_event = harness.tx_event;
        tokio::spawn(async move {
            if !matches!(rx_op.recv().await, Some(Op::SendMessage { .. })) {
                return;
            }
            let _ = tx_event
                .send(EngineEvent::TurnStarted {
                    turn_id: "mock_stream".to_string(),
                })
                .await;
            let _ = tx_event
                .send(EngineEvent::MessageStarted { index: 0 })
                .await;
            let _ = tx_event
                .send(EngineEvent::MessageDelta {
                    index: 0,
                    content: "streamed".to_string(),
                })
                .await;
            let _ = tx_event
                .send(EngineEvent::MessageComplete { index: 0 })
                .await;
            let _ = tx_event
                .send(EngineEvent::TurnComplete {
                    usage: Usage {
                        input_tokens: 4,
                        output_tokens: 2,
                        ..Usage::default()
                    },
                    last_request_input_tokens: None,
                    status: TurnOutcomeStatus::Completed,
                    error: None,
                })
                .await;
        });

        // Start the turn and consume events via the SSE endpoint.
        let turn_start: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads/{thread_id}/turns"))
            .json(&json!({ "prompt": "compatibility stream" }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let turn_id = turn_start["turn"]["id"]
            .as_str()
            .context("missing turn id")?
            .to_string();

        let _ = wait_for_terminal_turn_status(
            &client,
            addr,
            &thread_id,
            &turn_id,
            Duration::from_secs(2),
        )
        .await?;

        // Verify that the persisted events include the expected turn lifecycle events.
        let events = runtime_threads.events_since(&thread_id, None)?;
        assert!(
            events.iter().any(|ev| ev.event == "turn.started"),
            "expected turn.started event"
        );
        assert!(
            events.iter().any(|ev| ev.event == "turn.completed"),
            "expected turn.completed event"
        );

        // Verify the SSE endpoint returns event-stream content type.
        let events_resp = client
            .get(format!(
                "http://{addr}/v1/threads/{thread_id}/events?since_seq=0"
            ))
            .send()
            .await?
            .error_for_status()?;
        let content_type = events_resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.starts_with("text/event-stream"));

        handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn session_get_returns_404_for_missing_id() -> Result<()> {
        let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let resp = client
            .get(format!("http://{addr}/v1/sessions/nonexistent_id"))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn session_endpoints_reject_invalid_id() -> Result<()> {
        let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let get_resp = client
            .get(format!("http://{addr}/v1/sessions/invalid%20id"))
            .send()
            .await?;
        assert_eq!(get_resp.status(), StatusCode::BAD_REQUEST);

        let resume_resp = client
            .post(format!(
                "http://{addr}/v1/sessions/invalid%20id/resume-thread"
            ))
            .json(&json!({}))
            .send()
            .await?;
        assert_eq!(resume_resp.status(), StatusCode::BAD_REQUEST);

        let delete_resp = client
            .delete(format!("http://{addr}/v1/sessions/invalid%20id"))
            .send()
            .await?;
        assert_eq!(delete_resp.status(), StatusCode::BAD_REQUEST);

        handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn session_resume_thread_returns_404_for_missing_session() -> Result<()> {
        let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let resp = client
            .post(format!(
                "http://{addr}/v1/sessions/nonexistent_session/resume-thread"
            ))
            .json(&json!({}))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn session_resume_thread_creates_thread_from_saved_session() -> Result<()> {
        let root = std::env::temp_dir().join(format!("deepseek-session-resume-{}", Uuid::new_v4()));
        let sessions_dir = root.join("sessions");
        fs::create_dir_all(&sessions_dir)?;
        let session_id = "sess_test_resume";
        let saved_ws = root.join("saved_workspace");
        fs::create_dir_all(&saved_ws)?;
        let saved_ws_display = saved_ws.display().to_string();
        let session = json!({
            "schema_version": 1,
            "metadata": {
                "id": session_id,
                "title": "Test resume session",
                "created_at": "2025-01-01T00:00:00Z",
                "updated_at": "2025-01-01T00:10:00Z",
                "message_count": 2,
                "total_tokens": 100,
                "model": "deepseek-v4-pro",
                "workspace": saved_ws_display,
                "mode": "agent"
            },
            "messages": [
                {
                    "role": "user",
                    "content": [{ "type": "text", "text": "Hello, world!" }]
                },
                {
                    "role": "assistant",
                    "content": [{ "type": "text", "text": "Hello! How can I help you?" }]
                }
            ],
            "system_prompt": null
        });
        fs::write(
            sessions_dir.join(format!("{session_id}.json")),
            serde_json::to_string_pretty(&session)?,
        )?;

        let Some((addr, _runtime_threads, handle)) =
            spawn_test_server_with_root(root.clone(), sessions_dir.clone()).await?
        else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let resp = client
            .post(format!(
                "http://{addr}/v1/sessions/{session_id}/resume-thread"
            ))
            .json(&json!({ "model": "deepseek-v4-pro" }))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let resumed: serde_json::Value = resp.json().await?;
        assert_eq!(resumed["session_id"], session_id);
        assert_eq!(resumed["message_count"], 2);
        assert_eq!(resumed["state"], "seeding");

        let thread_id = resumed["thread_id"]
            .as_str()
            .context("missing resumed thread id")?;

        // Poll for seed completion since resume is now async
        for _ in 0..30 {
            let task_resp = client
                .get(format!(
                    "http://{addr}/v1/resume-tasks/{thread_id}"
                ))
                .send()
                .await?;
            if task_resp.status().is_success() {
                let task_state: serde_json::Value = task_resp.json().await?;
                if task_state["state"] == "ready" || task_state["state"] == "error" {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let detail: serde_json::Value = client
            .get(format!("http://{addr}/v1/threads/{thread_id}"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(detail["thread"]["id"], thread_id);
        assert_eq!(detail["turns"].as_array().map_or(0, Vec::len), 1);
        assert_eq!(detail["items"].as_array().map_or(0, Vec::len), 2);

        let got_ws = PathBuf::from(detail["thread"]["workspace"].as_str().unwrap());
        let canon_saved = fs::canonicalize(&saved_ws)?;
        let canon_got = fs::canonicalize(&got_ws)?;
        assert_eq!(canon_got, canon_saved);

        handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn session_delete_returns_404_for_missing_id() -> Result<()> {
        let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();
        let resp = client
            .delete(format!("http://{addr}/v1/sessions/nonexistent-id"))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        handle.abort();
        Ok(())
    }

    /// #561 / whalescale#255 — extra CORS origins from `RuntimeApiOptions`
    /// are added on top of the built-in defaults and propagate through to the
    /// `Access-Control-Allow-Origin` response header for preflight requests.
    /// Built-in defaults must keep working unchanged.
    #[tokio::test]
    async fn cors_layer_appends_extra_origins_and_keeps_defaults() -> Result<()> {
        // The cors_layer fn is the layer factory — exercise it through a
        // Router with a single trivial route so we can issue OPTIONS preflights
        // and observe the response headers.
        let extra = vec!["http://localhost:5173".to_string()];
        let layer = cors_layer(&extra);
        let router: Router = Router::new()
            .route("/probe", get(|| async { "ok" }))
            .layer(layer);

        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => return Ok(()),
            Err(err) => return Err(err.into()),
        };
        let addr = listener.local_addr()?;
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let client = reqwest::Client::new();

        // The user-supplied origin is allowed.
        let resp = client
            .request(reqwest::Method::OPTIONS, format!("http://{addr}/probe"))
            .header("Origin", "http://localhost:5173")
            .header("Access-Control-Request-Method", "GET")
            .send()
            .await?;
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("http://localhost:5173")
        );

        // A built-in default origin still works.
        let resp = client
            .request(reqwest::Method::OPTIONS, format!("http://{addr}/probe"))
            .header("Origin", "http://localhost:1420")
            .header("Access-Control-Request-Method", "GET")
            .send()
            .await?;
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("http://localhost:1420")
        );

        // An origin that's neither configured nor a default is rejected
        // (CorsLayer omits the Allow-Origin header on mismatch).
        let resp = client
            .request(reqwest::Method::OPTIONS, format!("http://{addr}/probe"))
            .header("Origin", "http://malicious.example")
            .header("Access-Control-Request-Method", "GET")
            .send()
            .await?;
        assert!(
            resp.headers().get("access-control-allow-origin").is_none(),
            "non-allowed origin must not be echoed back"
        );

        handle.abort();
        Ok(())
    }

    /// #561 — invalid origins (non-ASCII, etc.) are skipped without aborting
    /// the layer build.
    #[test]
    fn cors_layer_skips_invalid_origins() {
        let extras = vec![
            "http://valid.example".to_string(),
            // Embedded NUL char makes `HeaderValue::from_str` fail.
            "http://invalid.example\0".to_string(),
            "  ".to_string(), // whitespace-only is dropped
        ];
        // Should not panic.
        let _ = cors_layer(&extras);
    }

    /// #562 / whalescale#256 — `PATCH /v1/threads/{id}` accepts the new
    /// fields (allow_shell, trust_mode, auto_approve, model, mode, title,
    /// system_prompt). Each is independently optional; an empty string clears
    /// `title` / `system_prompt` back to None.
    #[tokio::test]
    async fn patch_thread_accepts_extended_field_set() -> Result<()> {
        let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let created: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads"))
            .json(&json!({
                "model": "deepseek-v4-flash",
                "mode": "agent"
            }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let thread_id = created["id"]
            .as_str()
            .context("missing thread id")?
            .to_string();

        // Patch every new field at once.
        let patched: serde_json::Value = client
            .patch(format!("http://{addr}/v1/threads/{thread_id}"))
            .json(&json!({
                "allow_shell": true,
                "trust_mode": true,
                "auto_approve": true,
                "model": "deepseek-v4-pro",
                "mode": "yolo",
                "title": "Whalescale UI test thread",
                "system_prompt": "You are a useful assistant."
            }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        assert_eq!(patched["allow_shell"], true);
        assert_eq!(patched["trust_mode"], true);
        assert_eq!(patched["auto_approve"], true);
        assert_eq!(patched["model"], "deepseek-v4-pro");
        assert_eq!(patched["mode"], "yolo");
        assert_eq!(patched["title"], "Whalescale UI test thread");
        assert_eq!(patched["system_prompt"], "You are a useful assistant.");

        // Empty string clears title back to None.
        let cleared: serde_json::Value = client
            .patch(format!("http://{addr}/v1/threads/{thread_id}"))
            .json(&json!({ "title": "" }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert!(
            cleared["title"].is_null() || !cleared.as_object().unwrap().contains_key("title"),
            "empty title must serialize as None: {cleared:?}"
        );

        // Empty patch (no fields) is still rejected.
        let empty = client
            .patch(format!("http://{addr}/v1/threads/{thread_id}"))
            .json(&json!({}))
            .send()
            .await?;
        assert_eq!(empty.status(), StatusCode::BAD_REQUEST);

        // Empty model is rejected (validation).
        let bad_model = client
            .patch(format!("http://{addr}/v1/threads/{thread_id}"))
            .json(&json!({ "model": "  " }))
            .send()
            .await?;
        assert_eq!(bad_model.status(), StatusCode::BAD_REQUEST);

        handle.abort();
        Ok(())
    }

    /// #563 / whalescale#260 — `archived_only=true` returns archived-only
    /// (no active threads), distinct from `include_archived=true` which
    /// returns both.
    #[tokio::test]
    async fn list_threads_archived_only_filter_matches_only_archived() -> Result<()> {
        let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        // Two threads — keep one active, archive the other.
        let active: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads"))
            .json(&json!({}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let active_id = active["id"].as_str().unwrap().to_string();

        let archived: serde_json::Value = client
            .post(format!("http://{addr}/v1/threads"))
            .json(&json!({}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let archived_id = archived["id"].as_str().unwrap().to_string();

        client
            .patch(format!("http://{addr}/v1/threads/{archived_id}"))
            .json(&json!({ "archived": true }))
            .send()
            .await?
            .error_for_status()?;

        // Default (active only) → only the unarchived one.
        let active_list: serde_json::Value = client
            .get(format!("http://{addr}/v1/threads"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let ids: Vec<&str> = active_list
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["id"].as_str())
            .collect();
        assert!(ids.contains(&active_id.as_str()));
        assert!(!ids.contains(&archived_id.as_str()));

        // archived_only=true → only the archived one.
        let archived_list: serde_json::Value = client
            .get(format!("http://{addr}/v1/threads?archived_only=true"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let ids: Vec<&str> = archived_list
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["id"].as_str())
            .collect();
        assert_eq!(ids, vec![archived_id.as_str()]);

        // archived_only=true takes precedence over include_archived=true.
        let archived_list: serde_json::Value = client
            .get(format!(
                "http://{addr}/v1/threads?include_archived=true&archived_only=true"
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let ids: Vec<&str> = archived_list
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["id"].as_str())
            .collect();
        assert_eq!(ids, vec![archived_id.as_str()]);

        // Same filter works on the summary endpoint.
        let summary: serde_json::Value = client
            .get(format!(
                "http://{addr}/v1/threads/summary?archived_only=true&limit=10"
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let summary_ids: Vec<&str> = summary
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["id"].as_str())
            .collect();
        assert_eq!(summary_ids, vec![archived_id.as_str()]);

        handle.abort();
        Ok(())
    }

    /// #564 / whalescale#261 — `GET /v1/usage` aggregates per-turn token +
    /// cost data. With no threads the response is well-formed and totals are
    /// zero with empty buckets (never a 404).
    #[tokio::test]
    async fn usage_endpoint_returns_empty_aggregation_for_fresh_store() -> Result<()> {
        let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
            return Ok(());
        };
        let client = reqwest::Client::new();

        let body: serde_json::Value = client
            .get(format!("http://{addr}/v1/usage"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(body["group_by"], "day");
        assert_eq!(body["totals"]["input_tokens"], 0);
        assert_eq!(body["totals"]["output_tokens"], 0);
        assert_eq!(body["totals"]["turns"], 0);
        assert!(
            body["buckets"].as_array().unwrap().is_empty(),
            "buckets must be empty when no turns exist: {body}"
        );

        // group_by query options are validated.
        let bad_group = client
            .get(format!("http://{addr}/v1/usage?group_by=galaxy"))
            .send()
            .await?;
        assert_eq!(bad_group.status(), StatusCode::BAD_REQUEST);

        // Each accepted group_by value succeeds.
        for gb in ["day", "model", "provider", "thread"] {
            let resp = client
                .get(format!("http://{addr}/v1/usage?group_by={gb}"))
                .send()
                .await?;
            assert!(resp.status().is_success(), "group_by={gb} failed: {resp:?}");
        }

        // Bad ISO-8601 timestamp rejected.
        let bad_since = client
            .get(format!("http://{addr}/v1/usage?since=not-a-date"))
            .send()
            .await?;
        assert_eq!(bad_since.status(), StatusCode::BAD_REQUEST);

        // since > until rejected.
        let inverted = client
            .get(format!(
                "http://{addr}/v1/usage?since=2030-01-02T00:00:00Z&until=2030-01-01T00:00:00Z"
            ))
            .send()
            .await?;
        assert_eq!(inverted.status(), StatusCode::BAD_REQUEST);

        handle.abort();
        Ok(())
    }
}