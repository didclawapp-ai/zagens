//! Session list/detail and resume-thread seeding (R-003 A4.5).

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{Mutex, Semaphore};

use crate::runtime_threads::CreateThreadRequest;
use crate::session_manager::SavedSession;

use zagens_runtime_api::{
    ApiError, ResumeSessionResponse, SessionDetailResponse, SessionsListResponse,
};

use super::RuntimeApiState;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ResumeTaskState {
    thread_id: String,
    session_id: String,
    state: String,
    items_written: usize,
    items_total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ResumeTaskTracker {
    tasks: Arc<Mutex<HashMap<String, ResumeTaskState>>>,
    seed_gate: Arc<Semaphore>,
}

impl ResumeTaskTracker {
    pub(crate) fn new() -> Self {
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

#[derive(Debug, Deserialize)]
pub(crate) struct ResumeSessionRequest {
    model: Option<String>,
    mode: Option<String>,
    #[serde(default)]
    task_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SessionsQuery {
    limit: Option<usize>,
    search: Option<String>,
}

pub(crate) async fn list_sessions(
    State(state): State<RuntimeApiState>,
    Query(query): Query<SessionsQuery>,
) -> Result<Json<SessionsListResponse>, ApiError> {
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
    Ok(Json(SessionsListResponse { sessions }))
}

pub(crate) async fn get_session(
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

pub(crate) async fn resume_session_thread(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<ResumeSessionRequest>,
) -> Result<(StatusCode, Json<ResumeSessionResponse>), ApiError> {
    let t0 = std::time::Instant::now();
    let manager = state.shared_session_manager.clone();
    let session = tokio::task::spawn_blocking({
        let id = id.clone();
        move || -> Result<SavedSession, std::io::Error> { manager.load_session(&id) }
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
    let task_type = crate::task_type::resolve_task_type(req.task_type.as_deref(), &workspace, None);

    // Reuse the persisted runtime thread when it still has events so Zagens can
    // replay tool cards and thinking after app restart (instead of seeding a blank thread).
    if let Some(ref stored_tid) = session.metadata.runtime_thread_id {
        let stored_tid = stored_tid.trim();
        if !stored_tid.is_empty() {
            match state.runtime_threads.load_thread_sync(stored_tid) {
                Ok(_) => {
                    let has_events = state
                        .runtime_threads
                        .events_since_async(stored_tid, Some(0))
                        .await
                        .map(|events| !events.is_empty())
                        .unwrap_or(false);
                    if has_events {
                        eprintln!(
                            "[resume-session] reusing runtime thread {stored_tid} (events present)"
                        );
                        if let Ok(thread) = state.runtime_threads.load_thread_sync(stored_tid) {
                            if let Some(ref turn_id) = thread.latest_turn_id {
                                super::kernel_replay::log_kernel_replay_for_turn(turn_id);
                            }
                        }
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
                    eprintln!(
                        "[resume-session] linked thread {stored_tid} has no events — seeding new thread"
                    );
                }
                Err(e) => {
                    eprintln!(
                        "[resume-session] linked thread {stored_tid} not on disk ({e}) — seeding new thread"
                    );
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

    state.resume_tracker.register(&tid, &id, msg_count).await;

    let tracker = state.resume_tracker.clone();
    let threads = state.runtime_threads.clone();
    tokio::spawn(async move {
        let _permit = tracker.acquire_seed_permit().await;

        let seed_result = tokio::task::spawn_blocking({
            let rt = tokio::runtime::Handle::current();
            let tid_clone = tid.clone();
            move || {
                rt.block_on(async { threads.seed_thread_from_messages(&tid_clone, &msgs).await })
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
                eprintln!("[resume-session] seed failed, thread={}: {}", tid_log, err,);
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

pub(crate) async fn delete_session(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<StatusCode, ApiError> {
    let manager = state.shared_session_manager.clone();
    tokio::task::spawn_blocking({
        let id = id.clone();
        move || -> Result<(), std::io::Error> { manager.delete_session(&id) }
    })
    .await
    .map_err(|e| ApiError::internal(format!("delete session task panicked: {e}")))?
    .map_err(|e| map_session_err(&id, e, "delete"))?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn get_resume_task(
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
                    crate::models::ContentBlock::ToolUse {
                        id, name, input, ..
                    } => {
                        json!({
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": input,
                        })
                    }
                    crate::models::ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                        ..
                    } => {
                        json!({
                            "type": "tool_result",
                            "tool_use_id": tool_use_id,
                            "content": content,
                            "is_error": is_error,
                        })
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

pub(crate) fn map_session_err(id: &str, err: std::io::Error, action: &str) -> ApiError {
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
