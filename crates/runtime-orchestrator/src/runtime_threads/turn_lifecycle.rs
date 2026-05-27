//! Turn start / monitor wiring (R-003 A4.6).

use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use deepseek_core::engine::TurnEnginePort;
use serde_json::json;
use uuid::Uuid;

use super::active::{ActiveTurnState, touch_lru};
use super::engine_host::{RuntimeThreadHost, spawn_turn_monitor};
use super::manager::RuntimeThreadManager;
use super::thread_crud::SUMMARY_LIMIT;
use super::types::*;
use super::{EditLastTurnRequest, StartTurnRequest, summarize_text};

pub async fn start_turn<P, R, H>(
    mgr: &RuntimeThreadManager<P, R>,
    host: &H,
    thread_id: &str,
    req: StartTurnRequest,
) -> Result<TurnRecord>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
    H: RuntimeThreadHost<P, R> + 'static,
{
    let prompt = req.prompt.trim().to_string();

    let mut req = req;
    if let Some(ref intent) = req.route_intent {
        let rules = mgr.routing_rules.lock().await;
        if let Some(rule) = rules.iter().find(|r| r.intent.eq_ignore_ascii_case(intent)) {
            req.model = Some(rule.model.clone());
        }
    }

    let mut thread = mgr.get_thread(thread_id).await?;
    let engine = host.ensure_engine_loaded(&thread).await?;

    {
        let active = mgr.active.lock().await;
        if let Some(active_thread) = active.engines.get(thread_id)
            && active_thread.active_turn.is_some()
        {
            bail!("Thread already has an active turn");
        }
    }

    let now = Utc::now();
    let turn_id = format!("turn_{}", &Uuid::new_v4().to_string()[..8]);
    let mut turn = TurnRecord {
        schema_version: super::CURRENT_RUNTIME_SCHEMA_VERSION,
        id: turn_id.clone(),
        thread_id: thread_id.to_string(),
        status: RuntimeTurnStatus::InProgress,
        input_summary: req
            .input_summary
            .clone()
            .unwrap_or_else(|| summarize_text(&prompt, SUMMARY_LIMIT)),
        created_at: now,
        started_at: Some(now),
        ended_at: None,
        duration_ms: None,
        usage: None,
        last_request_input_tokens: None,
        error: None,
        item_ids: Vec::new(),
        steer_count: 0,
    };

    let user_item_id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
    let user_item = TurnItemRecord {
        schema_version: super::CURRENT_RUNTIME_SCHEMA_VERSION,
        id: user_item_id.clone(),
        turn_id: turn_id.clone(),
        kind: TurnItemKind::UserMessage,
        status: TurnItemLifecycleStatus::Completed,
        summary: summarize_text(&prompt, SUMMARY_LIMIT),
        detail: Some(prompt.clone()),
        metadata: None,
        artifact_refs: Vec::new(),
        started_at: Some(now),
        ended_at: Some(now),
    };

    turn.item_ids.push(user_item_id.clone());
    thread.latest_turn_id = Some(turn_id.clone());
    thread.updated_at = now;

    {
        let store = mgr.store.clone();
        let user_item = user_item.clone();
        let turn = turn.clone();
        let thread = thread.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            store.save_item(&user_item)?;
            store.save_turn(&turn)?;
            store.save_thread(&thread)?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow!("save turn items panicked: {e}"))??;
    }

    mgr.emit_event(
        thread_id,
        Some(&turn_id),
        None,
        "turn.started",
        json!({ "turn": turn.clone() }),
    )
    .await?;
    mgr.emit_event(
        thread_id,
        Some(&turn_id),
        Some(&user_item_id),
        "item.started",
        json!({ "item": user_item.clone() }),
    )
    .await?;
    mgr.emit_event(
        thread_id,
        Some(&turn_id),
        Some(&user_item_id),
        "item.completed",
        json!({ "item": user_item }),
    )
    .await?;

    {
        let mut active = mgr.active.lock().await;
        let Some(state) = active.engines.get_mut(thread_id) else {
            bail!("Thread engine not loaded");
        };
        state.active_turn = Some(ActiveTurnState {
            turn_id: turn_id.clone(),
            interrupt_requested: false,
            auto_approve: req.auto_approve.unwrap_or(thread.auto_approve),
            trust_mode: req.trust_mode.unwrap_or(thread.trust_mode),
        });
        touch_lru(&mut active.lru, thread_id);
    }

    let start_params = host
        .prepare_start_turn_params(&thread, &req, &prompt)
        .await?;
    engine
        .start_turn(start_params)
        .await
        .map_err(|e| anyhow!("Failed to start turn: {e}"))?;

    spawn_turn_monitor(
        Arc::new(host.clone()),
        thread_id.to_string(),
        turn_id.clone(),
        engine,
        mgr.cancel_token.clone(),
        "turn",
    );

    Ok(turn)
}

pub async fn edit_last_turn<P, R, H>(
    mgr: &RuntimeThreadManager<P, R>,
    host: &H,
    thread_id: &str,
    req: EditLastTurnRequest,
) -> Result<TurnRecord>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
    H: RuntimeThreadHost<P, R> + 'static,
{
    let content = req.content.trim().to_string();
    if content.is_empty() {
        bail!("content is required");
    }

    {
        let active = mgr.active.lock().await;
        if let Some(active_thread) = active.engines.get(thread_id)
            && active_thread.active_turn.is_some()
        {
            bail!("Thread already has an active turn");
        }
    }

    let thread = mgr.get_thread(thread_id).await?;
    let engine = host.ensure_engine_loaded(&thread).await?;
    let truncated = engine
        .truncate_before_last_user_message()
        .await
        .context("truncate before last user message")?;
    if !truncated {
        bail!("No user message to edit");
    }

    start_turn(
        mgr,
        host,
        thread_id,
        StartTurnRequest {
            prompt: content,
            input_summary: None,
            model: req.model,
            mode: req.mode,
            allow_shell: req.allow_shell,
            trust_mode: req.trust_mode,
            auto_approve: req.auto_approve,
            route_intent: req.route_intent,
            temperature: req.temperature,
            top_p: req.top_p,
            max_tokens: req.max_tokens,
        },
    )
    .await
}
