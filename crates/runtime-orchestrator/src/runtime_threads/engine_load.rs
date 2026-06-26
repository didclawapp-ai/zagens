//! Generic engine load: cache hit, host spawn, session sync, LRU (D16 E1-b phase 4).

use anyhow::{Result, anyhow};

use crate::engine::{EngineHandle, Op};
use crate::models::{Message, SystemPrompt};

use super::active::{ActiveThreadState, enforce_lru_capacity, touch_lru};
use super::engine_host::RuntimeThreadHost;
use super::manager::RuntimeThreadManager;
use super::persist::reconstruct_messages_for_store;
use super::types::ThreadRecord;

pub async fn ensure_engine_loaded<P, R, H>(
    mgr: &RuntimeThreadManager<P, R>,
    host: &H,
    thread: &ThreadRecord,
) -> Result<EngineHandle<P, R>>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
    H: RuntimeThreadHost<P, R>,
{
    {
        let mut active = mgr.active.lock().await;
        if let Some(engine) = active
            .engines
            .get(thread.id.as_str())
            .map(|state| state.engine.clone())
        {
            touch_lru(&mut active.lru, &thread.id);
            return Ok(engine);
        }
    }

    let engine = host.spawn_engine_for_thread(thread).await?;

    let store = mgr.store.clone();
    let thread_id = thread.id.clone();
    let session_messages = tokio::task::spawn_blocking(move || -> Result<Vec<Message>> {
        let turns = store.list_turns_for_thread(&thread_id)?;
        reconstruct_messages_for_store(&store, &turns)
    })
    .await
    .map_err(|e| anyhow!("ensure_engine_loaded panicked: {e}"))??;

    let sys_prompt = thread
        .system_prompt
        .as_ref()
        .map(|s| SystemPrompt::Text(s.clone()));
    if !session_messages.is_empty() || sys_prompt.is_some() {
        engine
            .send(Op::SyncSession {
                messages: session_messages,
                system_prompt: sys_prompt,
                model: thread.model.clone(),
                workspace: thread.workspace.clone(),
            })
            .await
            .map_err(|e| anyhow!("Failed to sync thread session: {e}"))?;
    }

    let mut active = mgr.active.lock().await;
    let evicted = enforce_lru_capacity(&mut active, mgr.manager_cfg.max_active_threads);
    active.engines.insert(
        thread.id.clone(),
        ActiveThreadState {
            engine: engine.clone(),
            active_turn: None,
            pending_config_refresh: false,
        },
    );
    touch_lru(&mut active.lru, &thread.id);
    drop(active);
    for handle in evicted {
        let _ = handle.send(Op::Shutdown).await;
    }
    Ok(engine)
}
