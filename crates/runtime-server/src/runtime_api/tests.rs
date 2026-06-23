//! Integration tests for the runtime HTTP API (R-003 A4.5).

use super::skills::validate_skill_directory_name;
use super::stream::map_compat_stream_event;
use super::*;
use crate::automation_manager::AutomationManager;
use crate::config::{Config, DEFAULT_TEXT_MODEL};
use crate::core::events::{Event as EngineEvent, TurnOutcomeStatus};
use crate::core::ops::Op;
use crate::models::Usage;
use crate::runtime_threads::{
    RuntimeEventRecord, RuntimeThreadManager, RuntimeThreadManagerConfig,
    SharedRuntimeThreadManager,
};
use crate::session_manager::SessionManager;
use crate::task_manager::{TaskManager, TaskManagerConfig};
use anyhow::{Context, Result, bail};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::Sse;
use axum::{Router, routing::get};
use futures_util::StreamExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::convert::Infallible;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
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
    let runtime_data_dir = root.join("runtime");
    fs::create_dir_all(&runtime_data_dir)?;
    // Ignore workspace `DEEPSEEK_RUNTIME_DIR` (R-015 baseline / local dev) so each
    // test server gets an isolated store.
    let manager_cfg = RuntimeThreadManagerConfig {
        data_dir: runtime_data_dir.clone(),
        task_data_dir: runtime_data_dir,
        max_active_threads: 8,
        http_approval_timeout_secs: 120,
    };
    let runtime_threads: SharedRuntimeThreadManager = Arc::new(RuntimeThreadManager::open(
        config,
        PathBuf::from("."),
        manager_cfg,
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

    let shared_mcp_pool = Arc::new(tokio::sync::Mutex::new(
        crate::mcp::McpPool::from_config_path(&root.join("mcp.json"))
            .unwrap_or_else(|_| crate::mcp::McpPool::new(crate::mcp::McpConfig::default())),
    ));
    crate::mcp_shared::install_shared_mcp_pool(Arc::clone(&shared_mcp_pool));

    let state = RuntimeApiState::new(
        Config::default(),
        PathBuf::from("."),
        manager,
        runtime_threads.clone(),
        Vec::new(),
        root.join("mcp.json"),
        automations,
        runtime_token,
        0,
        Arc::new(token_fp),
        shared_sm,
        ResumeTaskTracker::new(),
        shared_mcp_pool,
    );
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
    let root = std::env::temp_dir().join(format!("zagens-runtime-api-{}", Uuid::new_v4()));
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
    assert_eq!(
        health["event_schema_version"],
        crate::runtime_threads::CURRENT_EVENT_SCHEMA_VERSION
    );

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
    let root = std::env::temp_dir().join(format!("zagens-runtime-api-{}", Uuid::new_v4()));
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

    let backtrack_err = client
        .post(format!(
            "http://{addr}/v1/threads/{thread_id}/fork-at-user-message"
        ))
        .json(&json!({ "depth_from_tail": 0 }))
        .send()
        .await?;
    assert!(!backtrack_err.status().is_success());

    seed_test_turns_with_user_messages(
        &runtime_threads,
        &thread_id,
        &["first user turn", "second user turn"],
    )?;

    let backtrack: serde_json::Value = client
        .post(format!(
            "http://{addr}/v1/threads/{thread_id}/fork-at-user-message"
        ))
        .json(&json!({ "depth_from_tail": 1 }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let backtrack_thread_id = backtrack["thread"]["id"]
        .as_str()
        .context("missing backtrack fork id")?;
    assert_ne!(backtrack_thread_id, thread_id);
    assert_eq!(backtrack["original_user_text"], "first user turn");

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
                            step_count: 0,
                            tool_names: vec![],
                            end_reason: None,
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
                            step_count: 0,
                            tool_names: vec![],
                            end_reason: None,
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

    let _ =
        wait_for_terminal_turn_status(&client, addr, &thread_id, &turn_id, Duration::from_secs(2))
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
    let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
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
                step_count: 0,
                tool_names: vec![],
                end_reason: None,
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

    let _ =
        wait_for_terminal_turn_status(&client, addr, &thread_id, &turn_id, Duration::from_secs(2))
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
                step_count: 0,
                tool_names: vec![],
                end_reason: None,
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

    let terminal =
        wait_for_terminal_turn_status(&client, addr, &thread_id, &turn_id, Duration::from_secs(3))
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
                step_count: 0,
                tool_names: vec![],
                end_reason: None,
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

    let _ =
        wait_for_terminal_turn_status(&client, addr, &thread_id, &turn_id, Duration::from_secs(2))
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
            .get(format!("http://{addr}/v1/resume-tasks/{thread_id}"))
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

/// D7 C3 — SQLite session must round-trip `runtime_thread_id` and resume reuses the linked thread.
#[tokio::test]
async fn session_resume_reuses_runtime_thread_when_sqlite_has_link() -> Result<()> {
    use crate::models::{ContentBlock, Message};
    use crate::session_manager::SessionManager;

    let root =
        std::env::temp_dir().join(format!("deepseek-session-sqlite-link-{}", Uuid::new_v4()));
    let sessions_dir = root.join("sessions");
    fs::create_dir_all(&sessions_dir)?;

    let Some((addr, runtime_threads, handle)) =
        spawn_test_server_with_root(root.clone(), sessions_dir.clone()).await?
    else {
        return Ok(());
    };

    assert!(
        sessions_dir.join("sessions.db").exists(),
        "SessionManager should open SQLite at sessions.db"
    );

    let client = reqwest::Client::new();
    let created: serde_json::Value = client
        .post(format!("http://{addr}/v1/threads"))
        .json(&json!({ "model": "deepseek-v4-pro" }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let thread_id = created["id"]
        .as_str()
        .context("missing thread id")?
        .to_string();

    let messages = vec![
        Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: "Hello from persisted session".to_string(),
                cache_control: None,
            }],
        },
        Message {
            role: "assistant".to_string(),
            content: vec![ContentBlock::Text {
                text: "Hi there".to_string(),
                cache_control: None,
            }],
        },
    ];
    runtime_threads
        .seed_thread_from_messages(&thread_id, &messages)
        .await?;

    let persist: serde_json::Value = client
        .post(format!(
            "http://{addr}/v1/threads/{thread_id}/persist-session"
        ))
        .json(&json!({}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let session_id = persist["session_id"]
        .as_str()
        .context("missing session_id from persist-session")?;

    let sm = SessionManager::new(sessions_dir.clone())?;
    let loaded = sm.load_session(session_id)?;
    assert_eq!(
        loaded.metadata.runtime_thread_id.as_deref(),
        Some(thread_id.as_str()),
        "C1: runtime_thread_id must survive SQLite save/load"
    );

    let resume_resp = client
        .post(format!(
            "http://{addr}/v1/sessions/{session_id}/resume-thread"
        ))
        .json(&json!({ "model": "deepseek-v4-pro" }))
        .send()
        .await?;
    assert_eq!(
        resume_resp.status(),
        StatusCode::OK,
        "linked thread with events should resume synchronously (ready), not 202 seeding"
    );
    let resumed: serde_json::Value = resume_resp.json().await?;
    assert_eq!(resumed["state"], "ready");
    assert_eq!(resumed["thread_id"], thread_id);
    assert_eq!(resumed["session_id"], session_id);

    handle.abort();
    Ok(())
}

/// Regression: calling `persist-session` repeatedly on the same thread —
/// with no `session_id`, or with a stale/blank one, as happens when the
/// web UI loses the threadId→sessionId binding during parallel-session
/// detach/reattach — must NOT create a new session each time. The backend
/// resolves the existing session by `runtime_thread_id` and updates it in
/// place, preserving the original title.
#[tokio::test]
async fn persist_session_does_not_duplicate_on_repeated_calls() -> Result<()> {
    use crate::models::{ContentBlock, Message};

    let root = std::env::temp_dir().join(format!("deepseek-persist-no-dup-{}", Uuid::new_v4()));
    let sessions_dir = root.join("sessions");
    fs::create_dir_all(&sessions_dir)?;

    let Some((addr, runtime_threads, handle)) =
        spawn_test_server_with_root(root.clone(), sessions_dir.clone()).await?
    else {
        return Ok(());
    };

    let client = reqwest::Client::new();
    let created: serde_json::Value = client
        .post(format!("http://{addr}/v1/threads"))
        .json(&json!({ "model": "deepseek-v4-pro" }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let thread_id = created["id"]
        .as_str()
        .context("missing thread id")?
        .to_string();

    // Seed an initial user turn so there is something to persist.
    let messages = vec![Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text: "First prompt in this thread".to_string(),
            cache_control: None,
        }],
    }];
    runtime_threads
        .seed_thread_from_messages(&thread_id, &messages)
        .await?;

    // First persist: no sid → creates the session and records the link.
    let first: serde_json::Value = client
        .post(format!(
            "http://{addr}/v1/threads/{thread_id}/persist-session"
        ))
        .json(&json!({}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let first_session_id = first["session_id"]
        .as_str()
        .context("missing session_id on first persist")?
        .to_string();

    // Second persist: still no sid — must resolve to the SAME session,
    // not create a new one.
    let second: serde_json::Value = client
        .post(format!(
            "http://{addr}/v1/threads/{thread_id}/persist-session"
        ))
        .json(&json!({}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(
        second["session_id"].as_str(),
        Some(first_session_id.as_str()),
        "repeated persist with blank sid must reuse the existing session, not duplicate it"
    );

    // Third persist: pass a foreign/blank sid — must still resolve back to
    // the thread's own session, never create a new one and never overwrite
    // an unrelated session.
    let foreign_sid = Uuid::new_v4().to_string();
    let third: serde_json::Value = client
        .post(format!(
            "http://{addr}/v1/threads/{thread_id}/persist-session"
        ))
        .json(&json!({ "session_id": foreign_sid }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(
        third["session_id"].as_str(),
        Some(first_session_id.as_str()),
        "persist with a foreign sid must fall back to the thread's own session, not create or overwrite another"
    );

    // Verify on disk: exactly one session linked to this thread.
    let sm = SessionManager::new(sessions_dir.clone())?;
    let linked = sm
        .find_session_id_by_runtime_thread_id(&thread_id)?
        .context("expected a session linked to the thread")?;
    assert_eq!(linked, first_session_id);

    // And the foreign sid we tried to inject must not exist on disk.
    assert!(
        sm.load_session(&foreign_sid).is_err(),
        "foreign sid must not have been created as a new session"
    );

    handle.abort();
    Ok(())
}

/// When the client passes a valid session id that exists but has no
/// `runtime_thread_id` yet (legacy row or first persist still in flight),
/// persist must update that row in place — not create a duplicate sidebar entry.
#[tokio::test]
async fn persist_session_claims_orphan_session_hint() -> Result<()> {
    use crate::models::{ContentBlock, Message};
    use crate::session_manager::{SessionManager, create_saved_session_with_mode};

    let root = std::env::temp_dir().join(format!("deepseek-persist-orphan-{}", Uuid::new_v4()));
    let sessions_dir = root.join("sessions");
    fs::create_dir_all(&sessions_dir)?;

    let Some((addr, runtime_threads, handle)) =
        spawn_test_server_with_root(root.clone(), sessions_dir.clone()).await?
    else {
        return Ok(());
    };

    let client = reqwest::Client::new();
    let created: serde_json::Value = client
        .post(format!("http://{addr}/v1/threads"))
        .json(&json!({ "model": "deepseek-v4-pro" }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let thread_id = created["id"]
        .as_str()
        .context("missing thread id")?
        .to_string();

    let messages = vec![Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text: "Follow-up prompt title".to_string(),
            cache_control: None,
        }],
    }];
    runtime_threads
        .seed_thread_from_messages(&thread_id, &messages)
        .await?;

    // Legacy sidebar row: exists but not linked to the runtime thread yet.
    let sm = SessionManager::new(sessions_dir.clone())?;
    let orphan =
        create_saved_session_with_mode(&messages, "deepseek-v4-pro", &root, 0, None, Some("agent"));
    let orphan_id = orphan.metadata.id.clone();
    sm.save_session(&orphan)?;

    let resp: serde_json::Value = client
        .post(format!(
            "http://{addr}/v1/threads/{thread_id}/persist-session"
        ))
        .json(&json!({ "session_id": orphan_id }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(
        resp["session_id"].as_str(),
        Some(orphan_id.as_str()),
        "orphan session hint must be claimed, not duplicated"
    );

    let linked = sm
        .find_session_id_by_runtime_thread_id(&thread_id)?
        .context("thread must be linked after persist")?;
    assert_eq!(linked, orphan_id);
    assert_eq!(sm.list_sessions()?.len(), 1, "must not create a second row");

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

/// A3.4 — HTTP errors expose unified taxonomy fields (`category`, `class`, `hint`, `retryable`).
#[tokio::test]
async fn api_error_payload_includes_taxonomy_fields() -> Result<()> {
    let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "http://{addr}/v1/threads/thr_any/turns/turn_any/resolve-approval"
        ))
        .json(&json!({
            "tool_call_id": "t1",
            "decision": "maybe",
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json().await?;
    let err = body.get("error").context("missing error object")?;
    assert!(err.get("message").and_then(|v| v.as_str()).is_some());
    assert_eq!(err.get("category"), err.get("class"));
    assert!(err.get("hint").and_then(|v| v.as_str()).is_some());
    assert_eq!(err["retryable"], false);
    assert_eq!(err["retry_policy"], "not_retryable");
    assert!(err.get("severity").is_some());
    handle.abort();
    Ok(())
}

/// A+.7 — HTTP resolve-approval rejects invalid decision strings.
#[tokio::test]
async fn resolve_approval_rejects_invalid_decision() -> Result<()> {
    let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    // Bad decision string
    let resp = client
        .post(format!(
            "http://{addr}/v1/threads/thr_any/turns/turn_any/resolve-approval"
        ))
        .json(&json!({
            "tool_call_id": "t1",
            "decision": "maybe",
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    handle.abort();
    Ok(())
}

/// PR5 — HTTP sidecar: two threads can run turns concurrently (multi-window).
#[tokio::test]
async fn sidecar_parallel_turns_on_two_threads() -> Result<()> {
    let Some((addr, runtime_threads, handle)) = spawn_test_server().await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    let base = format!("http://{addr}");

    let thread_a: serde_json::Value = client
        .post(format!("{base}/v1/threads"))
        .json(&json!({"model": "deepseek-chat"}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let thread_b: serde_json::Value = client
        .post(format!("{base}/v1/threads"))
        .json(&json!({"model": "deepseek-chat"}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let thread_a_id = thread_a["id"].as_str().context("thread A id")?.to_string();
    let thread_b_id = thread_b["id"].as_str().context("thread B id")?.to_string();

    for thread_id in [&thread_a_id, &thread_b_id] {
        let harness = crate::core::engine::mock_engine_handle();
        runtime_threads
            .install_test_engine(thread_id, harness.handle.clone())
            .await?;
        let mut rx_op = harness.rx_op;
        let tx_event = harness.tx_event;
        tokio::spawn(async move {
            if !matches!(rx_op.recv().await, Some(Op::SendMessage { .. })) {
                return;
            }
            let _ = tx_event
                .send(EngineEvent::TurnComplete {
                    usage: Usage::default(),
                    last_request_input_tokens: None,
                    status: TurnOutcomeStatus::Completed,
                    error: None,
                    step_count: 0,
                    tool_names: vec![],
                    end_reason: None,
                })
                .await;
        });
    }

    let turn_a: serde_json::Value = client
        .post(format!("{base}/v1/threads/{thread_a_id}/turns"))
        .json(&json!({"prompt": "parallel A"}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let turn_b: serde_json::Value = client
        .post(format!("{base}/v1/threads/{thread_b_id}/turns"))
        .json(&json!({"prompt": "parallel B"}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let turn_a_id = turn_a["turn"]["id"]
        .as_str()
        .context("turn A id")?
        .to_string();
    let turn_b_id = turn_b["turn"]["id"]
        .as_str()
        .context("turn B id")?
        .to_string();

    let status_a = wait_for_terminal_turn_status(
        &client,
        addr,
        &thread_a_id,
        &turn_a_id,
        Duration::from_secs(3),
    )
    .await?;
    let status_b = wait_for_terminal_turn_status(
        &client,
        addr,
        &thread_b_id,
        &turn_b_id,
        Duration::from_secs(3),
    )
    .await?;
    assert_eq!(status_a, "completed");
    assert_eq!(status_b, "completed");

    let events_a = runtime_threads.events_since(&thread_a_id, None)?;
    let events_b = runtime_threads.events_since(&thread_b_id, None)?;
    assert!(events_a.iter().any(|ev| ev.event == "turn.completed"));
    assert!(events_b.iter().any(|ev| ev.event == "turn.completed"));

    handle.abort();
    Ok(())
}

fn take_sse_frame(buf: &mut Vec<u8>) -> Option<String> {
    let text = String::from_utf8_lossy(buf);
    let (idx, delim) = if let Some(i) = text.find("\n\n") {
        (i, 2usize)
    } else if let Some(i) = text.find("\r\n\r\n") {
        (i, 4usize)
    } else {
        return None;
    };
    let frame = text[..idx].to_string();
    buf.drain(..idx + delim);
    Some(frame)
}

async fn collect_sse_events_until(
    client: reqwest::Client,
    url: String,
    stop_event: &str,
    timeout: Duration,
) -> Result<Vec<String>> {
    let resp = client.get(url).send().await?.error_for_status()?;
    let mut stream = resp.bytes_stream();
    let mut buf = Vec::new();
    let mut names = Vec::new();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let wait = deadline.saturating_duration_since(tokio::time::Instant::now());
        if wait.is_zero() {
            break;
        }
        let next = tokio::time::timeout(wait, stream.next()).await;
        let chunk = match next {
            Ok(Some(Ok(bytes))) => bytes,
            Ok(Some(Err(err))) => return Err(err.into()),
            Ok(None) | Err(_) => break,
        };
        buf.extend_from_slice(&chunk);
        while let Some(frame) = take_sse_frame(&mut buf) {
            if frame.starts_with(':') || frame.trim().is_empty() {
                continue;
            }
            let (name, _) = parse_sse_frame(&frame)?;
            names.push(name.clone());
            if name == stop_event {
                return Ok(names);
            }
        }
        if buf.len() > 256 * 1024 {
            bail!("SSE buffer exceeded 256KB without stop event {stop_event}");
        }
    }
    Ok(names)
}

/// Multi-session — two overlapping live SSE consumers on distinct threads stay isolated.
#[tokio::test]
async fn parallel_sse_live_streams_filter_by_thread_id() -> Result<()> {
    let Some((addr, runtime_threads, handle)) = spawn_test_server().await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    let base = format!("http://{addr}");

    let thread_a: serde_json::Value = client
        .post(format!("{base}/v1/threads"))
        .json(&json!({"model": "deepseek-chat"}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let thread_b: serde_json::Value = client
        .post(format!("{base}/v1/threads"))
        .json(&json!({"model": "deepseek-chat"}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let thread_a_id = thread_a["id"].as_str().context("thread A id")?.to_string();
    let thread_b_id = thread_b["id"].as_str().context("thread B id")?.to_string();

    let harness_a = crate::core::engine::mock_engine_handle();
    runtime_threads
        .install_test_engine(&thread_a_id, harness_a.handle.clone())
        .await?;
    let (release_a, hold_a) = tokio::sync::oneshot::channel::<()>();
    let mut rx_a = harness_a.rx_op;
    let tx_a = harness_a.tx_event;
    tokio::spawn(async move {
        if !matches!(rx_a.recv().await, Some(Op::SendMessage { .. })) {
            return;
        }
        let _ = hold_a.await;
        let _ = tx_a
            .send(EngineEvent::TurnComplete {
                usage: Usage::default(),
                last_request_input_tokens: None,
                status: TurnOutcomeStatus::Completed,
                error: None,
                step_count: 0,
                tool_names: vec![],
                end_reason: None,
            })
            .await;
    });

    let harness_b = crate::core::engine::mock_engine_handle();
    runtime_threads
        .install_test_engine(&thread_b_id, harness_b.handle.clone())
        .await?;
    let mut rx_b = harness_b.rx_op;
    let tx_b = harness_b.tx_event;
    tokio::spawn(async move {
        if !matches!(rx_b.recv().await, Some(Op::SendMessage { .. })) {
            return;
        }
        let _ = tx_b
            .send(EngineEvent::TurnComplete {
                usage: Usage::default(),
                last_request_input_tokens: None,
                status: TurnOutcomeStatus::Completed,
                error: None,
                step_count: 0,
                tool_names: vec![],
                end_reason: None,
            })
            .await;
    });

    let url_a = format!("{base}/v1/threads/{thread_a_id}/events?since_seq=0");
    let url_b = format!("{base}/v1/threads/{thread_b_id}/events?since_seq=0");
    let client_a = client.clone();
    let client_b = client.clone();
    let coll_a = tokio::spawn(async move {
        collect_sse_events_until(client_a, url_a, "turn.completed", Duration::from_secs(6)).await
    });
    let coll_b = tokio::spawn(async move {
        collect_sse_events_until(client_b, url_b, "turn.completed", Duration::from_secs(6)).await
    });

    sleep(Duration::from_millis(40)).await;

    let _turn_a: serde_json::Value = client
        .post(format!("{base}/v1/threads/{thread_a_id}/turns"))
        .json(&json!({"prompt": "parallel SSE A"}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    sleep(Duration::from_millis(40)).await;

    let _turn_b: serde_json::Value = client
        .post(format!("{base}/v1/threads/{thread_b_id}/turns"))
        .json(&json!({"prompt": "parallel SSE B"}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let events_b = coll_b.await.context("collector B join")??;
    assert!(
        events_b.iter().any(|ev| ev == "turn.completed"),
        "thread B SSE must receive its own turn.completed, got {events_b:?}"
    );

    assert!(
        !coll_a.is_finished(),
        "thread A SSE should still be open while A turn is held"
    );

    release_a
        .send(())
        .map_err(|_| anyhow::anyhow!("release channel closed"))?;

    let events_a = coll_a.await.context("collector A join")??;
    assert!(
        events_a.iter().any(|ev| ev == "turn.completed"),
        "thread A SSE must receive its own turn.completed after release, got {events_a:?}"
    );

    handle.abort();
    Ok(())
}

async fn wait_for_approval_required_event(
    runtime_threads: &crate::runtime_threads::RuntimeThreadManager,
    thread_id: &str,
    tool_call_id: &str,
    timeout: std::time::Duration,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let events = runtime_threads.events_since(thread_id, None)?;
        if events.iter().any(|ev| {
            ev.event == "approval.required"
                && ev.payload.get("id").and_then(Value::as_str) == Some(tool_call_id)
        }) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            bail!("timed out waiting for approval.required tool={tool_call_id} thread={thread_id}");
        }
        sleep(std::time::Duration::from_millis(25)).await;
    }
}

async fn install_parallel_approval_mock(
    runtime_threads: &crate::runtime_threads::RuntimeThreadManager,
    thread_id: &str,
    tool_call_id: &'static str,
) -> Result<tokio::sync::oneshot::Sender<()>> {
    let harness = crate::core::engine::mock_engine_handle();
    runtime_threads
        .install_test_engine(thread_id, harness.handle.clone())
        .await?;
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let mut rx_op = harness.rx_op;
    let tx_event = harness.tx_event;
    let mut rx_approval = harness.rx_approval;
    tokio::spawn(async move { while rx_approval.recv().await.is_some() {} });
    tokio::spawn(async move {
        if !matches!(rx_op.recv().await, Some(Op::SendMessage { .. })) {
            return;
        }
        let _ = tx_event
            .send(EngineEvent::ApprovalRequired {
                approval_key: format!("key_{tool_call_id}"),
                id: tool_call_id.to_string(),
                tool_name: "exec_command".to_string(),
                description: "needs HTTP resolve".to_string(),
            })
            .await;
        let _ = release_rx.await;
        let _ = tx_event
            .send(EngineEvent::TurnComplete {
                usage: Usage::default(),
                last_request_input_tokens: None,
                status: TurnOutcomeStatus::Completed,
                error: None,
                step_count: 0,
                tool_names: vec![],
                end_reason: None,
            })
            .await;
    });
    Ok(release_tx)
}

/// A+.7 — HTTP sidecar: two concurrent pending approvals; resolve-approval is
/// scoped per thread/turn; each turn continues only after its own resolve.
#[tokio::test]
async fn sidecar_parallel_pending_approvals_resolve_then_continue() -> Result<()> {
    let Some((addr, runtime_threads, handle)) = spawn_test_server().await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    let base = format!("http://{addr}");

    let thread_a: serde_json::Value = client
        .post(format!("{base}/v1/threads"))
        .json(&json!({"model": "deepseek-chat", "auto_approve": false}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let thread_b: serde_json::Value = client
        .post(format!("{base}/v1/threads"))
        .json(&json!({"model": "deepseek-chat", "auto_approve": false}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let thread_a_id = thread_a["id"].as_str().context("thread A id")?.to_string();
    let thread_b_id = thread_b["id"].as_str().context("thread B id")?.to_string();

    let release_a =
        install_parallel_approval_mock(&runtime_threads, &thread_a_id, "tool_http_parallel_a")
            .await?;
    let release_b =
        install_parallel_approval_mock(&runtime_threads, &thread_b_id, "tool_http_parallel_b")
            .await?;

    let turn_a: serde_json::Value = client
        .post(format!("{base}/v1/threads/{thread_a_id}/turns"))
        .json(&json!({"prompt": "parallel approval A", "auto_approve": false}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let turn_b: serde_json::Value = client
        .post(format!("{base}/v1/threads/{thread_b_id}/turns"))
        .json(&json!({"prompt": "parallel approval B", "auto_approve": false}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let turn_a_id = turn_a["turn"]["id"]
        .as_str()
        .context("turn A id")?
        .to_string();
    let turn_b_id = turn_b["turn"]["id"]
        .as_str()
        .context("turn B id")?
        .to_string();

    wait_for_approval_required_event(
        &runtime_threads,
        &thread_a_id,
        "tool_http_parallel_a",
        std::time::Duration::from_secs(3),
    )
    .await?;
    wait_for_approval_required_event(
        &runtime_threads,
        &thread_b_id,
        "tool_http_parallel_b",
        std::time::Duration::from_secs(3),
    )
    .await?;

    let cross = client
        .post(format!(
            "{base}/v1/threads/{thread_b_id}/turns/{turn_b_id}/resolve-approval"
        ))
        .json(&json!({
            "tool_call_id": "tool_http_parallel_a",
            "decision": "approve",
        }))
        .send()
        .await?;
    assert_eq!(
        cross.status(),
        StatusCode::CONFLICT,
        "resolving thread A's tool on thread B must fail scope check"
    );

    let approve_a = client
        .post(format!(
            "{base}/v1/threads/{thread_a_id}/turns/{turn_a_id}/resolve-approval"
        ))
        .json(&json!({
            "tool_call_id": "tool_http_parallel_a",
            "decision": "approve",
        }))
        .send()
        .await?;
    let approve_a_status = approve_a.status();
    let approve_a_body = approve_a.text().await?;
    assert!(
        approve_a_status.is_success(),
        "approve_a: {approve_a_status} {approve_a_body}"
    );
    let _ = release_a.send(());

    let approve_b = client
        .post(format!(
            "{base}/v1/threads/{thread_b_id}/turns/{turn_b_id}/resolve-approval"
        ))
        .json(&json!({
            "tool_call_id": "tool_http_parallel_b",
            "decision": "approve",
        }))
        .send()
        .await?;
    assert!(approve_b.status().is_success());
    let _ = release_b.send(());

    let status_a = wait_for_terminal_turn_status(
        &client,
        addr,
        &thread_a_id,
        &turn_a_id,
        std::time::Duration::from_secs(5),
    )
    .await?;
    let status_b = wait_for_terminal_turn_status(
        &client,
        addr,
        &thread_b_id,
        &turn_b_id,
        std::time::Duration::from_secs(5),
    )
    .await?;
    assert_eq!(status_a, "completed");
    assert_eq!(status_b, "completed");

    handle.abort();
    Ok(())
}

/// A+.4 Sidecar contract test: health → create thread → start turn →
/// SSE stream subset → interrupt.  This is the minimal smoke test that any
/// L3 shell (TUI / Zagens) expects to pass.  The server is in-memory
/// (same `spawn_test_server` helper); the full `deepseek-tui serve
/// --http` binary-test variant lives under
/// `scripts/runtime-longrun-baseline.ps1`.
#[tokio::test]
async fn sidecar_contract_full_lifecycle() -> Result<()> {
    let Some((addr, _runtime_threads, handle)) = spawn_test_server().await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    let base = format!("http://{addr}");

    // 1. Health check
    let health: serde_json::Value = client
        .get(format!("{base}/health"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(health["status"], "ok");
    assert_eq!(
        health["event_schema_version"],
        crate::runtime_threads::CURRENT_EVENT_SCHEMA_VERSION
    );

    // 2. Create thread
    let thread: serde_json::Value = client
        .post(format!("{base}/v1/threads"))
        .json(&json!({"model": "deepseek-chat"}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let thread_id = thread["id"]
        .as_str()
        .context("missing thread id")?
        .to_string();

    // 3. Start turn (simulated via mock engine events)
    let turn: serde_json::Value = client
        .post(format!("{base}/v1/threads/{thread_id}/turns"))
        .json(&json!({
            "prompt": "contract test turn",
            "model": "deepseek-chat",
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let turn_id = turn["turn"]["id"]
        .as_str()
        .context("missing turn id")?
        .to_string();

    // 4. Subscribe to events (SSE) — verify we get at least
    //    turn.started within the timeout.
    let events_url = format!("{base}/v1/threads/{thread_id}/events?replay_only=1");
    let events_resp = client.get(&events_url).send().await?.error_for_status()?;
    let events_body = events_resp.text().await?;
    assert!(
        events_body.contains("event: turn.started")
            || events_body.contains("event: turn.completed"),
        "expected at least one SSE event in replay, got: {events_body:.200}"
    );

    // 5. Interrupt the turn (Zagens + TUI both use POST .../interrupt — §12.4 #2)
    let interrupt_resp = client
        .post(format!(
            "{base}/v1/threads/{thread_id}/turns/{turn_id}/interrupt"
        ))
        .send()
        .await?;
    assert!(
        interrupt_resp.status().is_success() || interrupt_resp.status() == StatusCode::CONFLICT,
        "interrupt should succeed or 409 if already terminal; got {}",
        interrupt_resp.status()
    );

    handle.abort();
    Ok(())
}

fn seed_test_turns_with_user_messages(
    manager: &crate::runtime_threads::RuntimeThreadManager,
    thread_id: &str,
    user_texts: &[&str],
) -> Result<()> {
    use crate::runtime_threads::{
        CURRENT_EVENT_SCHEMA_VERSION, RuntimeTurnStatus, TurnItemKind, TurnItemLifecycleStatus,
        TurnItemRecord, TurnRecord,
    };
    use chrono::Utc;

    let base = Utc::now();
    for (offset, text) in user_texts.iter().enumerate() {
        let created_at = base + chrono::Duration::milliseconds(offset as i64);
        let turn_id = format!("turn_http_seed_{offset}");
        let user_item_id = format!("item_user_{offset}");
        let asst_item_id = format!("item_asst_{offset}");
        manager.store.save_item(&TurnItemRecord {
            schema_version: CURRENT_EVENT_SCHEMA_VERSION,
            id: user_item_id.clone(),
            turn_id: turn_id.clone(),
            kind: TurnItemKind::UserMessage,
            status: TurnItemLifecycleStatus::Completed,
            summary: (*text).to_string(),
            detail: Some((*text).to_string()),
            metadata: None,
            artifact_refs: Vec::new(),
            started_at: Some(created_at),
            ended_at: Some(created_at),
        })?;
        manager.store.save_item(&TurnItemRecord {
            schema_version: CURRENT_EVENT_SCHEMA_VERSION,
            id: asst_item_id.clone(),
            turn_id: turn_id.clone(),
            kind: TurnItemKind::AgentMessage,
            status: TurnItemLifecycleStatus::Completed,
            summary: format!("reply {offset}"),
            detail: Some(format!("reply {offset}")),
            metadata: None,
            artifact_refs: Vec::new(),
            started_at: Some(created_at),
            ended_at: Some(created_at),
        })?;
        manager.store.save_turn(&TurnRecord {
            schema_version: CURRENT_EVENT_SCHEMA_VERSION,
            id: turn_id.clone(),
            thread_id: thread_id.to_string(),
            status: RuntimeTurnStatus::Completed,
            input_summary: (*text).to_string(),
            created_at,
            started_at: Some(created_at),
            ended_at: Some(created_at),
            duration_ms: Some(0),
            usage: None,
            last_request_input_tokens: None,
            error: None,
            item_ids: vec![user_item_id, asst_item_id],
            steer_count: 0,
        })?;
    }
    Ok(())
}
