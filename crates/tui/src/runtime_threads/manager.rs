//! Active engine threads, turns, and live event broadcast (R-003 A4.6).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config::{Config, DEFAULT_TEXT_MODEL};
use crate::context_snapshot::{ThreadContextSnapshot, build_thread_context_snapshot};
use crate::core::coherence::CoherenceState;
use crate::core::engine::EngineHandle;
use deepseek_core::engine::{StartTurnParams, TurnEnginePort};
use crate::core::events::{Event as EngineEvent, TurnOutcomeStatus};
use crate::core::ops::Op;
use crate::models::{ContentBlock, Message, SystemPrompt, Usage};
use crate::tools::subagent::SubAgentStatus;
use crate::agent_surface::AppMode;

use super::active::{
    enforce_lru_capacity, touch_lru, ActiveThreadState, ActiveThreads, ActiveTurnState,
    RuntimeApprovalDecision,
};
use super::events::collect_agent_rebind_hints;
use super::persist::{
    duration_ms, reconstruct_messages_for_store, write_json_atomic, RuntimeThreadStore,
};
use super::types::*;
use super::routing::{load_routing_rules, save_routing_rules};
use super::{
    summarize_text, CompactThreadRequest, CreateThreadRequest, RoutingRule,
    RuntimeThreadManagerConfig, StartTurnRequest, SteerTurnRequest, ThreadDetail,
    ThreadListFilter, UpdateThreadRequest, UsageAggregation, UsageGroupBy, AgentRebindHint,
    CURRENT_RUNTIME_SCHEMA_VERSION, EVENT_CHANNEL_CAPACITY, RUNTIME_RESTART_REASON,
    SUMMARY_LIMIT,
};

pub type SharedRuntimeThreadManager = Arc<RuntimeThreadManager>;

/// Manages active engine threads, lifecycle, and event persistence.
///
/// # Lock ordering invariant
///
/// Two `Mutex`es exist across this module:
/// - `RuntimeThreadStore::state` — protects the monotonic event sequence counter.
/// - `RuntimeThreadManager::active` — protects the set of loaded engine handles.
///
/// **No code path holds both locks simultaneously.** The `state` lock is only
/// acquired inside `RuntimeThreadStore::append_event` (where it is explicitly
/// dropped before any I/O) and `current_seq`. All `emit_event` calls (which
/// call `append_event`) happen *after* `active` has been released. If you add
/// new code that touches both, always acquire `state` before `active` to
/// preserve a consistent ordering.
#[derive(Clone)]
pub struct RuntimeThreadManager {
    pub(crate) config: Config,
    pub(crate) workspace: PathBuf,
    pub(crate) store: RuntimeThreadStore,
    pub(crate) active: Arc<Mutex<ActiveThreads>>,
    event_tx: broadcast::Sender<RuntimeEventRecord>,
    pub(crate) manager_cfg: RuntimeThreadManagerConfig,
    pub(crate) cancel_token: CancellationToken,
    pub(crate) task_manager: Arc<StdMutex<Option<crate::task_manager::SharedTaskManager>>>,
    pub(crate) automations: Arc<StdMutex<Option<crate::automation_manager::SharedAutomationManager>>>,
    pub(crate) routing_rules: Arc<Mutex<Vec<RoutingRule>>>,
    pub(crate) routing_rules_path: PathBuf,
    checklist_cache: Arc<StdMutex<HashMap<String, String>>>,
    scratchpad_status_cache: Arc<StdMutex<HashMap<String, ScratchpadStatusCacheEntry>>>,
}

#[derive(Clone)]
struct ScratchpadStatusCacheEntry {
    fetched_at: Instant,
    status: Option<serde_json::Value>,
}

const SCRATCHPAD_STATUS_CACHE_TTL: Duration = Duration::from_secs(2);

impl RuntimeThreadManager {
    pub fn open(
        config: Config,
        workspace: PathBuf,
        manager_cfg: RuntimeThreadManagerConfig,
    ) -> Result<Self> {
        let store = RuntimeThreadStore::open(manager_cfg.data_dir.clone())?;
        Self::open_with_store(config, workspace, manager_cfg, store)
    }

    pub(crate) fn open_with_store(
        config: Config,
        workspace: PathBuf,
        manager_cfg: RuntimeThreadManagerConfig,
        store: RuntimeThreadStore,
    ) -> Result<Self> {
        let (event_tx, _event_rx) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let routing_rules_path = manager_cfg.data_dir.join("routing_rules.json");
        let routing_rules = load_routing_rules(&routing_rules_path).unwrap_or_default();
        let manager = Self {
            config,
            workspace,
            store,
            active: Arc::new(Mutex::new(ActiveThreads::default())),
            event_tx,
            manager_cfg,
            cancel_token: CancellationToken::new(),
            task_manager: Arc::new(StdMutex::new(None)),
            automations: Arc::new(StdMutex::new(None)),
            routing_rules: Arc::new(Mutex::new(routing_rules)),
            routing_rules_path,
            checklist_cache: Arc::new(StdMutex::new(HashMap::new())),
            scratchpad_status_cache: Arc::new(StdMutex::new(HashMap::new())),
        };
        manager.recover_interrupted_state()?;
        let active_ids: Vec<String> = manager
            .store
            .list_threads()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|t| t.scratchpad_run_id)
            .collect();
        crate::scratchpad::cleanup::cleanup_stale_scratchpads(
            &manager.workspace,
            &manager.config.scratchpad_config(),
            &active_ids,
        );
        Ok(manager)
    }

    /// Read-only audit scratchpad progress for a thread (B5).
    pub fn get_thread_scratchpad_status(
        &self,
        thread_id: &str,
    ) -> Result<Option<serde_json::Value>> {
        if let Ok(cache) = self.scratchpad_status_cache.lock() {
            if let Some(entry) = cache.get(thread_id) {
                if entry.fetched_at.elapsed() < SCRATCHPAD_STATUS_CACHE_TTL {
                    return Ok(entry.status.clone());
                }
            }
        }
        let mut thread = self.load_thread_sync(thread_id)?;
        let run_id = thread
            .scratchpad_run_id
            .clone()
            .or_else(|| crate::scratchpad::discover_scratchpad_run_id_for_ui(&thread.workspace));
        let Some(run_id) = run_id else {
            return Ok(None);
        };
        if thread.scratchpad_run_id.as_deref() != Some(run_id.as_str()) {
            thread.scratchpad_run_id = Some(run_id.clone());
            thread.updated_at = Utc::now();
            let store = self.store.clone();
            let thread_to_save = thread.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = tokio::task::spawn_blocking(move || store.save_thread(&thread_to_save))
                        .await;
                });
            } else {
                let _ = self.store.save_thread(&thread);
            }
        }
        let store = crate::scratchpad::try_open_store(
            &thread.workspace,
            Some(run_id.as_str()),
            Some(&thread.id),
            thread.task_id.as_deref(),
        );
        let Some(mut status) = store.and_then(|s| s.build_status().ok()) else {
            return Ok(None);
        };
        let checklist_json = self.get_thread_checklist(thread_id);
        crate::scratchpad::ui_status::enrich_status_for_thread_ui(
            &mut status,
            checklist_json.as_deref(),
        );
        let out = Some(status);
        if let Ok(mut cache) = self.scratchpad_status_cache.lock() {
            cache.insert(
                thread_id.to_string(),
                ScratchpadStatusCacheEntry {
                    fetched_at: Instant::now(),
                    status: out.clone(),
                },
            );
        }
        Ok(out)
    }

    /// Return the cached checklist snapshot for a thread (for Zagens WebView panel).
    pub fn get_thread_checklist(&self, thread_id: &str) -> Option<String> {
        if let Ok(cache) = self.checklist_cache.lock() {
            if let Some(json) = cache.get(thread_id) {
                return Some(json.clone());
            }
        }
        let thread = self.store.load_thread(thread_id).ok()?;
        let json = thread
            .checklist_snapshot
            .and_then(|v| serde_json::to_string(&v).ok())?;
        if let Ok(mut cache) = self.checklist_cache.lock() {
            cache.insert(thread_id.to_string(), json.clone());
        }
        Some(json)
    }

    pub(crate) fn persist_thread_checklist(&self, thread_id: &str, checklist_json: &str) {
        if let Ok(mut cache) = self.checklist_cache.lock() {
            cache.insert(thread_id.to_string(), checklist_json.to_string());
        }
        let snapshot: Option<serde_json::Value> = serde_json::from_str(checklist_json).ok();
        let store = self.store.clone();
        let thread_id_owned = thread_id.to_string();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let thread_id = thread_id_owned;
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(mut thread) = store.load_thread(&thread_id) {
                        thread.checklist_snapshot = snapshot;
                        thread.updated_at = Utc::now();
                        if store.save_thread(&thread).is_err() {
                            tracing::warn!(%thread_id, "failed to persist checklist snapshot on thread");
                        }
                    }
                })
                .await;
            });
        } else if let Ok(mut thread) = self.store.load_thread(thread_id) {
            thread.checklist_snapshot = snapshot;
            thread.updated_at = Utc::now();
            if self.store.save_thread(&thread).is_err() {
                tracing::warn!(thread_id, "failed to persist checklist snapshot on thread");
            }
        }
    }

    /// Zagens panel channel (C): push checklist snapshot on the live SSE stream (B-channel fallback).
    pub(crate) async fn emit_panel_checklist(&self, thread_id: &str, turn_id: &str) -> Result<()> {
        let Some(json_str) = self.get_thread_checklist(thread_id) else {
            return Ok(());
        };
        let checklist = serde_json::from_str::<Value>(&json_str).unwrap_or_else(|_| {
            json!({ "raw": json_str })
        });
        self.emit_event(
            thread_id,
            Some(turn_id),
            None,
            "panel.checklist",
            json!({ "checklist": checklist }),
        )
        .await?;
        Ok(())
    }

    /// Zagens panel channel (C): push audit scratchpad status on SSE.
    pub(crate) async fn emit_panel_scratchpad(&self, thread_id: &str, turn_id: &str) -> Result<()> {
        let status = self.get_thread_scratchpad_status(thread_id)?;
        if let Some(scratchpad) = status {
            self.emit_event(
                thread_id,
                Some(turn_id),
                None,
                "panel.scratchpad",
                json!({ "scratchpad": scratchpad }),
            )
            .await?;
        }
        Ok(())
    }

    /// Zagens panel channel (C): push context usage snapshot on SSE.
    pub(crate) async fn emit_panel_context(&self, thread_id: &str, turn_id: &str) -> Result<()> {
        match self.get_thread_context(thread_id).await {
            Ok(context) => {
                let snapshot = serde_json::to_value(&context)?;
                self.emit_event(
                    thread_id,
                    Some(turn_id),
                    None,
                    "panel.context",
                    json!({ "context": snapshot }),
                )
                .await?;
            }
            Err(err) => {
                tracing::debug!(
                    thread_id,
                    %err,
                    "panel.context skipped (context query failed)"
                );
            }
        }
        Ok(())
    }

    pub(crate) fn scratchpad_tool_needs_panel_push(name: &str) -> bool {
        name.starts_with("scratchpad_")
    }

    pub(crate) fn checklist_tool_needs_panel_push(name: &str) -> bool {
        matches!(
            name,
            "checklist_write"
                | "checklist_add"
                | "checklist_update"
                | "todo_write"
                | "todo_add"
                | "todo_update"
        )
    }

    /// Attach the durable task manager so model-visible task tools work inside
    /// runtime thread turns as well as interactive TUI turns.
    pub fn attach_task_manager(&self, task_manager: crate::task_manager::SharedTaskManager) {
        if let Ok(mut slot) = self.task_manager.lock() {
            *slot = Some(task_manager);
        }
    }

    /// Attach the automation manager for model-visible scheduling tools.
    pub fn attach_automation_manager(
        &self,
        automations: crate::automation_manager::SharedAutomationManager,
    ) {
        if let Ok(mut slot) = self.automations.lock() {
            *slot = Some(automations);
        }
    }

    #[allow(dead_code)] // Public API for external callers (runtime API, task manager)
    pub fn shutdown(&self) {
        self.cancel_token.cancel();
    }

    #[allow(dead_code)] // Public API for external callers
    pub fn is_shutdown(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    #[must_use]
    pub fn subscribe_events(&self) -> broadcast::Receiver<RuntimeEventRecord> {
        self.event_tx.subscribe()
    }

    pub(crate) async fn emit_event(
        &self,
        thread_id: &str,
        turn_id: Option<&str>,
        item_id: Option<&str>,
        event: impl Into<String>,
        payload: Value,
    ) -> Result<RuntimeEventRecord> {
        let record = self
            .store
            .append_event(thread_id, turn_id, item_id, event, payload)
            .await?;
        if let Err(e) = self.event_tx.send(record.clone()) {
            tracing::debug!(
                "Runtime event broadcast failed (no receivers or channel full): {}",
                e
            );
        }
        Ok(record)
    }

    /// Synchronous read — tests and blocking contexts only.
    pub fn events_since(
        &self,
        thread_id: &str,
        since_seq: Option<u64>,
    ) -> Result<Vec<RuntimeEventRecord>> {
        self.store.events_since(thread_id, since_seq)
    }

    /// Offloads SQLite/JSONL reads from the async runtime (A1.3).
    pub async fn events_since_async(
        &self,
        thread_id: &str,
        since_seq: Option<u64>,
    ) -> Result<Vec<RuntimeEventRecord>> {
        let store = self.store.clone();
        let thread_id = thread_id.to_string();
        tokio::task::spawn_blocking(move || store.events_since(&thread_id, since_seq))
            .await
            .map_err(|e| anyhow!("events_since join: {e}"))?
    }

    pub(crate) fn resolve_thread_workspace_path(manager_workspace: &Path, raw: &str) -> Result<PathBuf> {
        let trimmed = raw.trim();
        let candidate = if trimmed.is_empty() || trimmed == "." {
            manager_workspace.to_path_buf()
        } else {
            let p = PathBuf::from(trimmed);
            if p.is_absolute() {
                p
            } else {
                manager_workspace.join(p)
            }
        };
        let canon = fs::canonicalize(&candidate).with_context(|| {
            anyhow!(
                "workspace path does not exist or is not reachable: {}",
                candidate.display()
            )
        })?;
        let meta =
            fs::metadata(&canon).with_context(|| format!("workspace stat {}", canon.display()))?;
        if !meta.is_dir() {
            bail!("workspace path is not a directory: {}", canon.display());
        }
        Ok(canon)
    }

    pub(crate) async fn unload_idle_thread_engine(&self, thread_id: &str) -> Result<()> {
        let maybe_engine = {
            let mut active = self.active.lock().await;
            if let Some(st) = active.engines.get(thread_id)
                && st.active_turn.is_some()
            {
                bail!("thread has an active turn; finish or interrupt before rebinding workspace");
            }
            if let Some(idx) = active.lru.iter().position(|id| id.as_str() == thread_id) {
                active.lru.remove(idx);
            }
            active.engines.remove(thread_id).map(|s| s.engine)
        };
        if let Some(engine) = maybe_engine {
            let _ = engine.send(Op::Shutdown).await;
        }
        Ok(())
    }

    fn reconstruct_messages_from_turns(&self, turns: &[TurnRecord]) -> Result<Vec<Message>> {
        reconstruct_messages_for_store(&self.store, turns)
    }

    /// Serialised turn items → API messages plus approximate token totals for session files.
    pub fn export_thread_for_session_persist(
        &self,
        thread_id: &str,
    ) -> Result<(Vec<Message>, u64)> {
        let turns = self
            .store
            .list_turns_for_thread(thread_id)
            .with_context(|| format!("list turns for thread {thread_id}"))?;
        let mut total_tokens: u64 = 0;
        for t in &turns {
            if let Some(u) = &t.usage {
                total_tokens += u64::from(u.input_tokens) + u64::from(u.output_tokens);
                if let Some(r) = u.reasoning_tokens {
                    total_tokens += u64::from(r);
                }
                if let Some(rr) = u.reasoning_replay_tokens {
                    total_tokens += u64::from(rr);
                }
            }
        }
        let messages = self.reconstruct_messages_from_turns(&turns)?;
        Ok((messages, total_tokens))
    }

    pub async fn active_turn_flags(&self, thread_id: &str, turn_id: &str) -> Option<(bool, bool)> {
        let active = self.active.lock().await;
        let state = active.engines.get(thread_id)?;
        let turn = state.active_turn.as_ref()?;
        if turn.turn_id != turn_id {
            return None;
        }
        Some((turn.auto_approve, turn.trust_mode))
    }

    pub async fn resolve_approval(
        &self,
        thread_id: &str,
        turn_id: &str,
        tool_call_id: &str,
        approved: bool,
    ) -> Result<()> {
        let engine = {
            let mut active = self.active.lock().await;
            let pending = active
                .pending_approvals
                .remove(tool_call_id)
                .ok_or_else(|| anyhow!("no pending approval for {tool_call_id}"))?;
            if pending.thread_id != thread_id || pending.turn_id != turn_id {
                let expected_thread = pending.thread_id.clone();
                let expected_turn = pending.turn_id.clone();
                active
                    .pending_approvals
                    .insert(tool_call_id.to_string(), pending);
                bail!(
                    "pending approval scope mismatch for {tool_call_id}: expected thread {expected_thread} turn {expected_turn}, URL had thread {thread_id} turn {turn_id}"
                );
            }
            let state = active
                .engines
                .get(thread_id)
                .ok_or_else(|| anyhow!("engine not found for {thread_id}"))?;
            state.engine.clone()
        };

        if approved {
            engine.approve_tool_call(tool_call_id).await?;
        } else {
            engine.deny_tool_call(tool_call_id).await?;
        }
        Ok(())
    }

    pub(crate) fn approval_decision(
        auto_approve: bool,
        trust_mode: bool,
        requires_full_access: bool,
    ) -> RuntimeApprovalDecision {
        if !auto_approve {
            return RuntimeApprovalDecision::DenyTool;
        }
        if requires_full_access {
            if trust_mode {
                RuntimeApprovalDecision::RetryWithFullAccess
            } else {
                RuntimeApprovalDecision::DenyTool
            }
        } else {
            RuntimeApprovalDecision::ApproveTool
        }
    }

    fn recover_interrupted_state(&self) -> Result<()> {
        let now = Utc::now();
        let incomplete = self.store.list_incomplete_turns()?;
        let mut by_thread: HashMap<String, Vec<TurnRecord>> = HashMap::new();
        for turn in incomplete {
            by_thread
                .entry(turn.thread_id.clone())
                .or_default()
                .push(turn);
        }
        for turns in by_thread.values_mut() {
            turns.sort_by_key(|t| t.created_at);
        }

        for mut thread in self.store.list_threads()? {
            let Some(mut turns) = by_thread.remove(&thread.id) else {
                continue;
            };
            let mut thread_changed = false;
            for mut turn in turns.drain(..) {
                turn.status = RuntimeTurnStatus::Interrupted;
                turn.error = Some(RUNTIME_RESTART_REASON.to_string());
                turn.ended_at = Some(now);
                if let Some(started_at) = turn.started_at {
                    let elapsed = now.signed_duration_since(started_at);
                    turn.duration_ms = Some(elapsed.num_milliseconds().max(0) as u64);
                }
                self.store.save_turn(&turn)?;

                for item_id in &turn.item_ids {
                    let mut item = self.store.load_item(item_id)?;
                    if matches!(
                        item.status,
                        TurnItemLifecycleStatus::Queued | TurnItemLifecycleStatus::InProgress
                    ) {
                        item.status = TurnItemLifecycleStatus::Interrupted;
                        item.ended_at = Some(now);
                        self.store.save_item(&item)?;
                    }
                }

                thread.updated_at = now;
                thread_changed = true;
            }

            if thread_changed {
                self.store.save_thread(&thread)?;
            }
        }

        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn install_test_engine(
        &self,
        thread_id: &str,
        engine: EngineHandle,
    ) -> Result<()> {
        let _ = self.get_thread(thread_id).await?;
        let mut active = self.active.lock().await;
        active.engines.insert(
            thread_id.to_string(),
            ActiveThreadState {
                engine,
                active_turn: None,
            },
        );
        touch_lru(&mut active.lru, thread_id);
        Ok(())
    }
}
pub(crate) fn parse_mode(mode: &str) -> AppMode {
    match mode.trim().to_ascii_lowercase().as_str() {
        "plan" => AppMode::Plan,
        "yolo" => AppMode::Yolo,
        _ => AppMode::Agent,
    }
}

pub(crate) fn tool_kind_for_name(name: &str) -> TurnItemKind {
    let lower = name.to_ascii_lowercase();
    if lower == "exec_shell" || lower == "exec_shell_wait" || lower == "exec_shell_interact" {
        return TurnItemKind::CommandExecution;
    }
    if lower.contains("patch") || lower.contains("write") || lower.contains("edit") {
        return TurnItemKind::FileChange;
    }
    TurnItemKind::ToolCall
}

