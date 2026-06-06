//! Turn interrupt, steer, and manual compaction (R-003 A4.6).

use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::engine::Op;

use super::active::{ActiveTurnState, touch_lru};
use super::engine_host::{RuntimeThreadHost, spawn_turn_monitor};
use super::engine_load::ensure_engine_loaded;
use super::manager::RuntimeThreadManager;
use super::thread_crud::SUMMARY_LIMIT;
use super::types::*;
use super::{CompactThreadRequest, SteerTurnRequest, summarize_text};

impl<P, R> RuntimeThreadManager<P, R>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
{
    pub async fn interrupt_turn(&self, thread_id: &str, turn_id: &str) -> Result<TurnRecord> {
        {
            let interrupt_seq = self
                .store
                .allocate_session_input_seq(thread_id)
                .unwrap_or(0);
            let mut coordinators = self.coordinators.lock().await;
            coordinators.interrupt(thread_id, interrupt_seq);
        }

        {
            let mut active = self.active.lock().await;
            let Some(active_thread) = active.engines.get_mut(thread_id) else {
                bail!("Thread is not loaded");
            };
            let Some(active_turn) = active_thread.active_turn.as_mut() else {
                bail!("No active turn on thread {thread_id}");
            };
            if active_turn.turn_id != turn_id {
                bail!("Turn {turn_id} is not active on thread {thread_id}");
            }
            active_turn.interrupt_requested = true;
            active_thread.engine.cancel();
            touch_lru(&mut active.lru, thread_id);
        }

        self.emit_event(
            thread_id,
            Some(turn_id),
            None,
            "turn.interrupt_requested",
            json!({ "thread_id": thread_id, "turn_id": turn_id }),
        )
        .await?;

        self.store.load_turn(turn_id)
    }

    pub async fn steer_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
        req: SteerTurnRequest,
    ) -> Result<TurnRecord> {
        let prompt = req.prompt.trim().to_string();
        if prompt.is_empty() {
            bail!("prompt is required");
        }

        let admitted_seq = self.store.allocate_session_input_seq(thread_id)?;
        let admission = super::prompt_inbox::PromptAdmission {
            id: format!("inp_{}", &Uuid::new_v4().to_string()[..8]),
            thread_id: thread_id.to_string(),
            admitted_seq,
            prompt: prompt.clone(),
            delivery: super::prompt_inbox::PromptDelivery::Steer,
            time_created: Utc::now(),
            promoted_seq: None,
        };
        self.store.admit_session_input(&admission, None)?;

        let engine = {
            let mut active = self.active.lock().await;
            let engine = {
                let Some(active_thread) = active.engines.get_mut(thread_id) else {
                    bail!("Thread is not loaded");
                };
                let Some(active_turn) = active_thread.active_turn.as_mut() else {
                    bail!("No active turn on thread {thread_id}");
                };
                if active_turn.turn_id != turn_id {
                    bail!("Turn {turn_id} is not active on thread {thread_id}");
                }
                active_thread.engine.clone()
            };
            touch_lru(&mut active.lru, thread_id);
            engine
        };

        engine
            .steer(prompt.clone())
            .await
            .map_err(|e| anyhow!("Failed to steer turn: {e}"))?;

        let now = Utc::now();
        let mut turn = self.store.load_turn(turn_id)?;
        turn.steer_count = turn.steer_count.saturating_add(1);

        let item = TurnItemRecord {
            schema_version: super::CURRENT_RUNTIME_SCHEMA_VERSION,
            id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
            turn_id: turn_id.to_string(),
            kind: TurnItemKind::UserMessage,
            status: TurnItemLifecycleStatus::Completed,
            summary: summarize_text(&prompt, SUMMARY_LIMIT),
            detail: Some(prompt.clone()),
            metadata: None,
            artifact_refs: Vec::new(),
            started_at: Some(now),
            ended_at: Some(now),
        };
        turn.item_ids.push(item.id.clone());

        let promoted = self
            .emit_event(
                thread_id,
                Some(turn_id),
                Some(&item.id),
                "prompt.promoted",
                json!({ "admission": admission, "turn_id": turn_id }),
            )
            .await?;

        {
            let store = self.store.clone();
            let turn_clone = turn.clone();
            let item_clone = item.clone();
            let admission_id = admission.id.clone();
            let promoted_seq = promoted.seq;
            let turn_id_owned = turn_id.to_string();
            tokio::task::spawn_blocking(move || -> Result<()> {
                store.promote_session_input(&admission_id, promoted_seq, Some(&turn_id_owned))?;
                store.save_turn(&turn_clone)?;
                store.save_item(&item_clone)?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow!("save steer items panicked: {e}"))??;
        }

        self.emit_event(
            thread_id,
            Some(turn_id),
            Some(&item.id),
            "turn.steered",
            json!({
                "thread_id": thread_id,
                "turn_id": turn_id,
                "input": prompt,
            }),
        )
        .await?;
        self.emit_event(
            thread_id,
            Some(turn_id),
            Some(&item.id),
            "item.completed",
            json!({ "item": item }),
        )
        .await?;

        Ok(turn)
    }
}

pub async fn compact_thread<P, R, H>(
    mgr: &RuntimeThreadManager<P, R>,
    host: &H,
    thread_id: &str,
    req: CompactThreadRequest,
) -> Result<TurnRecord>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
    H: RuntimeThreadHost<P, R> + 'static,
{
    let mut thread = mgr.get_thread(thread_id).await?;
    let engine = ensure_engine_loaded(mgr, host, &thread).await?;

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
    let turn = TurnRecord {
        schema_version: super::CURRENT_RUNTIME_SCHEMA_VERSION,
        id: turn_id.clone(),
        thread_id: thread_id.to_string(),
        status: RuntimeTurnStatus::InProgress,
        input_summary: req
            .reason
            .as_deref()
            .map(|s| summarize_text(s, SUMMARY_LIMIT))
            .unwrap_or_else(|| "Manual context compaction".to_string()),
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
    thread.latest_turn_id = Some(turn_id.clone());
    thread.updated_at = now;
    {
        let store = mgr.store.clone();
        let turn_clone = turn.clone();
        let thread_clone = thread.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            store.save_turn(&turn_clone)?;
            store.save_thread(&thread_clone)?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow!("save compact turn panicked: {e}"))??;
    }

    {
        let mut active = mgr.active.lock().await;
        let Some(state) = active.engines.get_mut(thread_id) else {
            bail!("Thread engine not loaded");
        };
        state.active_turn = Some(ActiveTurnState {
            turn_id: turn_id.clone(),
            interrupt_requested: false,
            auto_approve: thread.auto_approve,
            trust_mode: thread.trust_mode,
        });
        touch_lru(&mut active.lru, thread_id);
    }

    mgr.emit_event(
        thread_id,
        Some(&turn_id),
        None,
        "turn.started",
        json!({ "turn": turn.clone(), "manual_compaction": true }),
    )
    .await?;

    engine
        .send(Op::CompactContext)
        .await
        .map_err(|e| anyhow!("Failed to trigger compaction: {e}"))?;

    spawn_turn_monitor(
        Arc::new(host.clone()),
        thread_id.to_string(),
        turn_id.clone(),
        engine,
        mgr.cancel_token.clone(),
        "compaction turn",
    );

    Ok(turn)
}
