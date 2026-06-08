//! Turn start / monitor wiring (R-003 A4.6) + P0/P1 coordinator + durable inbox.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;
use zagens_core::engine::TurnEnginePort;

use super::active::{ActiveTurnState, touch_lru};
use super::engine_host::{RuntimeThreadHost, spawn_turn_monitor};
use super::engine_load::ensure_engine_loaded;
use super::manager::RuntimeThreadManager;
use super::prompt_inbox::{PromptAdmission, PromptDelivery};
use super::thread_crud::SUMMARY_LIMIT;
use super::turn_coordinator::CoordinatorAction;
use super::types::*;
use super::{EditLastTurnRequest, StartTurnOutcome, StartTurnRequest, summarize_text};

pub async fn start_turn<P, R, H>(
    mgr: &RuntimeThreadManager<P, R>,
    host: &H,
    thread_id: &str,
    req: StartTurnRequest,
) -> Result<StartTurnOutcome>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
    H: RuntimeThreadHost<P, R> + 'static,
{
    let prompt = req.prompt.trim().to_string();
    if prompt.is_empty() {
        bail!("prompt is required");
    }

    let mut req = req;
    if let Some(ref intent) = req.route_intent {
        let rules = mgr.routing_rules.lock().await;
        if let Some(rule) = rules.iter().find(|r| r.intent.eq_ignore_ascii_case(intent)) {
            req.model = Some(rule.model.clone());
        }
    }

    let delivery = req.delivery.unwrap_or(PromptDelivery::Queue);
    let input_id = format!("inp_{}", &Uuid::new_v4().to_string()[..8]);
    let admitted_seq = mgr.store.allocate_session_input_seq(thread_id)?;
    let admission = PromptAdmission {
        id: input_id.clone(),
        thread_id: thread_id.to_string(),
        admitted_seq,
        prompt: prompt.clone(),
        delivery,
        time_created: Utc::now(),
        promoted_seq: None,
    };
    mgr.store.admit_session_input(&admission, Some(&req))?;

    let active_turn_id = {
        let active = mgr.active.lock().await;
        active
            .engines
            .get(thread_id)
            .and_then(|st| st.active_turn.as_ref().map(|t| t.turn_id.clone()))
    };

    if let Some(active_turn_id) = active_turn_id {
        if req.delivery != Some(PromptDelivery::Queue) {
            bail!("Thread already has an active turn");
        }
        let action = {
            let mut coordinators = mgr.coordinators.lock().await;
            coordinators.request_wake(thread_id, admitted_seq)
        };
        if action == CoordinatorAction::Suppressed {
            bail!("Queued prompt suppressed by interrupt boundary");
        }
        mgr.emit_event(
            thread_id,
            Some(&active_turn_id),
            None,
            "prompt.admitted",
            json!({ "admission": admission }),
        )
        .await?;
        let turn = mgr.store.load_turn(&active_turn_id)?;
        return Ok(StartTurnOutcome {
            turn,
            queued: Some(admission),
        });
    }

    {
        let mut coordinators = mgr.coordinators.lock().await;
        let _ = coordinators.request_run(thread_id);
    }

    let turn = start_turn_promoted(mgr, host, thread_id, &req, &prompt, &admission).await?;
    Ok(StartTurnOutcome { turn, queued: None })
}

pub async fn start_turn_promoted<P, R, H>(
    mgr: &RuntimeThreadManager<P, R>,
    host: &H,
    thread_id: &str,
    req: &StartTurnRequest,
    prompt: &str,
    admission: &PromptAdmission,
) -> Result<TurnRecord>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
    H: RuntimeThreadHost<P, R> + 'static,
{
    let mut thread = mgr.get_thread(thread_id).await?;
    let engine = ensure_engine_loaded(mgr, host, &thread).await?;

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
            .unwrap_or_else(|| summarize_text(prompt, SUMMARY_LIMIT)),
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
        summary: summarize_text(prompt, SUMMARY_LIMIT),
        detail: Some(prompt.to_string()),
        metadata: None,
        artifact_refs: Vec::new(),
        started_at: Some(now),
        ended_at: Some(now),
    };

    turn.item_ids.push(user_item_id.clone());
    thread.latest_turn_id = Some(turn_id.clone());
    thread.updated_at = now;

    let promoted_seq = mgr
        .store
        .append_event(
            thread_id,
            Some(&turn_id),
            Some(&user_item_id),
            "prompt.promoted",
            json!({ "admission": admission, "turn_id": turn_id }),
        )
        .await?
        .seq;

    {
        let store = mgr.store.clone();
        let user_item = user_item.clone();
        let turn = turn.clone();
        let thread = thread.clone();
        let admission_id = admission.id.clone();
        let turn_id_for_promote = turn_id.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            store.promote_session_input(&admission_id, promoted_seq, Some(&turn_id_for_promote))?;
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

    let start_params = host.prepare_start_turn_params(&thread, req, prompt).await?;
    if let Err(e) = engine.start_turn(start_params).await {
        rollback_failed_turn_start(mgr, thread_id, &turn_id, e.to_string()).await?;
        return Err(anyhow!("Failed to start turn: {e}"));
    }

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

/// After a turn completes, promote the next queued prompt if any.
pub async fn drain_queued_turn<P, R, H>(
    mgr: &RuntimeThreadManager<P, R>,
    host: &H,
    thread_id: &str,
) -> Result<()>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
    H: RuntimeThreadHost<P, R> + 'static,
{
    let follow_up = {
        let mut coordinators = mgr.coordinators.lock().await;
        coordinators.finish_drain(thread_id)
    };

    if follow_up.is_none() {
        return Ok(());
    }

    let Some((admission, stored_req)) = mgr.store.next_pending_queue(thread_id)? else {
        return Ok(());
    };

    let req = stored_req.unwrap_or(StartTurnRequest {
        prompt: admission.prompt.clone(),
        ..Default::default()
    });

    let _ = start_turn_promoted(mgr, host, thread_id, &req, &admission.prompt, &admission).await?;

    Ok(())
}

/// Clear in-memory `active_turn` and persist `Failed` when `engine.start_turn` never ran.
async fn rollback_failed_turn_start<P, R>(
    mgr: &RuntimeThreadManager<P, R>,
    thread_id: &str,
    turn_id: &str,
    err_msg: String,
) -> Result<()>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
{
    let now = Utc::now();
    let mut turn = mgr.store.load_turn(turn_id)?;
    turn.status = RuntimeTurnStatus::Failed;
    turn.error = Some(err_msg);
    turn.ended_at = Some(now);
    if let Some(started) = turn.started_at {
        turn.duration_ms = Some((now - started).num_milliseconds().max(0) as u64);
    }

    {
        let store = mgr.store.clone();
        let turn_clone = turn.clone();
        tokio::task::spawn_blocking(move || store.save_turn(&turn_clone))
            .await
            .map_err(|e| anyhow!("save failed turn panicked: {e}"))??;
    }

    mgr.emit_event(
        thread_id,
        Some(turn_id),
        None,
        "turn.completed",
        json!({ "turn": turn }),
    )
    .await?;

    {
        let mut active = mgr.active.lock().await;
        if let Some(state) = active.engines.get_mut(thread_id)
            && state
                .active_turn
                .as_ref()
                .is_some_and(|t| t.turn_id == turn_id)
        {
            state.active_turn = None;
        }
        touch_lru(&mut active.lru, thread_id);
    }

    {
        let mut coordinators = mgr.coordinators.lock().await;
        let _ = coordinators.finish_drain(thread_id);
    }

    Ok(())
}

pub async fn edit_last_turn<P, R, H>(
    mgr: &RuntimeThreadManager<P, R>,
    host: &H,
    thread_id: &str,
    req: EditLastTurnRequest,
) -> Result<StartTurnOutcome>
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
    let engine = ensure_engine_loaded(mgr, host, &thread).await?;
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
            delivery: None,
        },
    )
    .await
}
