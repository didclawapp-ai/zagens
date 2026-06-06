//! Thread CRUD, turns, snapshots, and per-thread workspace routes (R-003 A4.4).

use std::fs;
use std::path::{Component, Path, PathBuf};

use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::config::DEFAULT_TEXT_MODEL;
use crate::models::SystemPrompt;
use crate::runtime_threads::{
    CompactThreadRequest, CreateThreadRequest, EditLastTurnRequest, ForkAtUserMessageRequest,
    ForkAtUserMessageResponse, StartTurnRequest, SteerTurnRequest, ThreadDetail, ThreadListFilter,
    ThreadRecord, TurnItemKind, TurnRecord, UpdateThreadRequest,
};
use crate::session_manager::{SavedSession, create_saved_session_with_mode, update_session};
use crate::snapshot::SnapshotRepo;

use deepseek_runtime_api::{StartTurnResponse, ThreadSummary};

use super::{ApiError, RuntimeApiState, map_thread_err, truncate_text};

#[derive(Debug, Deserialize)]
pub(crate) struct ResolveApprovalRequest {
    tool_call_id: String,
    decision: String,
    #[serde(default)]
    remember_for_session: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ThreadsQuery {
    limit: Option<usize>,
    include_archived: Option<bool>,
    /// When `true`, returns archived threads only (overrides `include_archived`).
    /// Whalescale#260 / #563.
    archived_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ThreadSummaryQuery {
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

#[derive(Debug, Deserialize)]
pub(crate) struct PersistThreadSessionRequest {
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PersistThreadSessionResponse {
    session_id: String,
    message_count: usize,
}

/// Writes the thread's turn/item history to `~/.deepseek/sessions/*.json` (same format as TUI).
pub(crate) async fn persist_thread_session(
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
                    "thread has no messages to persist; wait for turn.completed".to_string()
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
                let mut session = update_session(existing, &messages, total_tokens, sys.as_ref());
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
pub(crate) async fn create_thread(
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

pub(crate) async fn list_threads(
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

pub(crate) async fn list_threads_summary(
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
            updated_at: thread.updated_at.to_rfc3339(),
            latest_turn_id: thread.latest_turn_id,
            latest_turn_status: latest_status,
        });
    }

    if summaries.len() > limit {
        summaries.truncate(limit);
    }

    Ok(Json(summaries))
}
const MAX_SNAPSHOT_LIST: usize = 100;
const MAX_WORKSPACE_FILE_BYTES: usize = 512 * 1024;

#[derive(Debug, Deserialize)]
pub(crate) struct BrowseWorkspaceQuery {
    /// Path relative to thread workspace (`/` or `\` optional; no `..`).
    #[serde(default)]
    path: String,
}

/// Browse a directory on disk by absolute workspace root (Composer path before a runtime thread exists).
#[derive(Debug, Deserialize)]
pub(crate) struct BrowseWorkspaceByRootQuery {
    workspace: String,
    #[serde(default)]
    path: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReadWorkspaceFileByRootQuery {
    workspace: String,
    path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct BrowseWorkspaceEntry {
    name: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BrowseWorkspaceResponse {
    workspace: String,
    /// Relative path using `/` (empty = root).
    path: String,
    entries: Vec<BrowseWorkspaceEntry>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SnapshotListEntryJson {
    /// 1-based, newest first (same as TUI `/restore N`).
    n: usize,
    id: String,
    label: String,
    timestamp: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct SnapshotsListResponse {
    workspace: String,
    snapshots: Vec<SnapshotListEntryJson>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RestoreSnapshotBody {
    /// 1-based index into newest-first list (`1` = latest).
    n: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct RestoreSnapshotResponse {
    restored: bool,
    label: String,
    id: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceFileResponse {
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

fn map_snapshot_io_err(workspace: &str, err: std::io::Error) -> ApiError {
    let msg = err.to_string();
    if msg.contains("could not lock config")
        || msg.contains("git init failed")
        || msg.contains("snapshot repo init timed out")
    {
        ApiError::bad_request(format!(
            "Workspace snapshot repository for {workspace} is locked or incomplete. \
             Retry in a moment; if it persists, remove the matching folder under \
             ~/.zagens/snapshots/ and reload this panel."
        ))
    } else {
        ApiError::bad_request(format!("snapshots unavailable for {workspace}: {msg}"))
    }
}

pub(crate) async fn list_thread_snapshots(
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
    let max_gb = state.config.snapshots_config().max_workspace_gb;
    let snapshots = tokio::task::spawn_blocking(move || {
        let repo = SnapshotRepo::open_or_init_with_max_gb(&ws, max_gb)?;
        repo.list(limit)
    })
    .await
    .map_err(|e| ApiError::internal(format!("snapshot task: {e}")))?
    .map_err(|e| map_snapshot_io_err(&workspace_display, e))?;
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

pub(crate) async fn restore_thread_snapshot(
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
    let max_gb = state.config.snapshots_config().max_workspace_gb;
    let restored = tokio::task::spawn_blocking(move || {
        let repo = SnapshotRepo::open_or_init_with_max_gb(&ws, max_gb)
            .map_err(|e| map_snapshot_io_err(&ws.display().to_string(), e))?;
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

pub(crate) async fn browse_thread_workspace(
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

pub(crate) async fn browse_workspace_by_root(
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

pub(crate) async fn read_workspace_file_by_root(
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

pub(crate) async fn read_thread_workspace_file(
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
pub(crate) async fn get_thread(
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

pub(crate) async fn update_thread(
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

pub(crate) async fn resume_thread(
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

pub(crate) async fn fork_thread(
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

pub(crate) async fn fork_thread_at_user_message(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<ForkAtUserMessageRequest>,
) -> Result<(StatusCode, Json<ForkAtUserMessageResponse>), ApiError> {
    let (thread, original_user_text) = state
        .runtime_threads
        .fork_at_user_message(&id, req.depth_from_tail)
        .await
        .map_err(map_thread_err)?;
    Ok((
        StatusCode::CREATED,
        Json(ForkAtUserMessageResponse {
            thread,
            original_user_text,
        }),
    ))
}

pub(crate) async fn start_thread_turn(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<StartTurnRequest>,
) -> Result<(StatusCode, Json<StartTurnResponse>), ApiError> {
    let outcome = state
        .runtime_threads
        .start_turn(&id, req)
        .await
        .map_err(map_thread_err)?;
    let thread = state
        .runtime_threads
        .get_thread(&id)
        .await
        .map_err(map_thread_err)?;
    let status = if outcome.queued.is_some() {
        StatusCode::ACCEPTED
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(StartTurnResponse {
            thread,
            turn: outcome.turn,
            queued: outcome.queued,
        }),
    ))
}

pub(crate) async fn edit_last_thread_turn(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<EditLastTurnRequest>,
) -> Result<(StatusCode, Json<StartTurnResponse>), ApiError> {
    let outcome = state
        .runtime_threads
        .edit_last_turn(&id, req)
        .await
        .map_err(map_thread_err)?;
    let thread = state
        .runtime_threads
        .get_thread(&id)
        .await
        .map_err(map_thread_err)?;
    Ok((
        StatusCode::CREATED,
        Json(StartTurnResponse {
            thread,
            turn: outcome.turn,
            queued: outcome.queued,
        }),
    ))
}

pub(crate) async fn steer_thread_turn(
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

pub(crate) async fn resolve_approval(
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
        .resolve_approval(
            &id,
            &turn_id,
            &req.tool_call_id,
            approved,
            req.remember_for_session,
        )
        .await
        .map_err(map_thread_err)?;
    Ok(Json(json!({
        "ok": true,
        "tool_call_id": req.tool_call_id,
        "decision": req.decision,
        "turn_id": turn_id,
    })))
}

pub(crate) async fn interrupt_thread_turn(
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

pub(crate) async fn compact_thread(
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
        Json(StartTurnResponse {
            thread,
            turn,
            queued: None,
        }),
    ))
}

pub(crate) async fn get_thread_harness_task_graph(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let threads = state.runtime_threads.clone();
    let id = id.clone();
    let graph = threads
        .get_thread_harness_task_graph(&id)
        .await
        .map_err(map_thread_err)?;
    Ok(Json(graph))
}

pub(crate) async fn get_thread_harness_cycles(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let threads = state.runtime_threads.clone();
    let id = id.clone();
    let cycles = threads
        .get_thread_harness_cycles(&id)
        .await
        .map_err(map_thread_err)?;
    Ok(Json(cycles))
}

pub(crate) async fn get_thread_checklist(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let threads = state.runtime_threads.clone();
    let id = id.clone();
    let json_str = tokio::task::spawn_blocking(move || threads.get_thread_checklist(&id))
        .await
        .map_err(|e| ApiError::internal(format!("checklist task panicked: {e}")))?;
    match json_str {
        Some(s) => {
            let parsed: Value = serde_json::from_str(&s).unwrap_or(Value::Null);
            Ok(Json(parsed))
        }
        None => Ok(Json(Value::Null)),
    }
}

pub(crate) async fn get_thread_scratchpad_status(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let threads = state.runtime_threads.clone();
    let id = id.clone();
    let status = tokio::task::spawn_blocking(move || threads.get_thread_scratchpad_status(&id))
        .await
        .map_err(|e| ApiError::internal(format!("scratchpad status task panicked: {e}")))?
        .map_err(map_thread_err)?;
    Ok(Json(status.unwrap_or(Value::Null)))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct InitThreadScratchpadRequest {
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    areas: Option<Vec<serde_json::Value>>,
}

pub(crate) async fn init_thread_scratchpad(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<InitThreadScratchpadRequest>,
) -> Result<Json<Value>, ApiError> {
    let threads = state.runtime_threads.clone();
    let id = id.clone();
    let run_id = body.run_id;
    let scope = body.scope;
    let areas = body.areas;
    let status = tokio::task::spawn_blocking(move || {
        threads.init_thread_scratchpad(&id, run_id.as_deref(), scope.as_deref(), areas.as_deref())
    })
    .await
    .map_err(|e| ApiError::internal(format!("scratchpad init task panicked: {e}")))?
    .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(status))
}

pub(crate) async fn get_thread_context(
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
