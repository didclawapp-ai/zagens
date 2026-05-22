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

use crate::config::{Config, DEFAULT_TEXT_MODEL, MAX_SUBAGENTS};
use crate::context_snapshot::{ThreadContextSnapshot, build_thread_context_snapshot};
use crate::core::coherence::CoherenceState;
use crate::core::engine::{EngineConfig, EngineHandle, spawn_engine};
use crate::core::events::{Event as EngineEvent, TurnOutcomeStatus};
use crate::core::ops::Op;
use crate::models::{ContentBlock, Message, SystemPrompt, Usage};
use crate::tools::plan::new_shared_plan_state;
use crate::tools::subagent::SubAgentStatus;
use crate::tools::todo::new_shared_todo_list;
use crate::tui::app::AppMode;

use super::events::collect_agent_rebind_hints;
use super::persist::{
    duration_ms, reconstruct_messages_for_store, write_json_atomic, RuntimeThreadStore,
};
use super::types::*;
use super::{
    summarize_text, CompactThreadRequest, CreateThreadRequest, RoutingRule, RoutingRulesDoc,
    RuntimeThreadManagerConfig, StartTurnRequest, SteerTurnRequest, ThreadDetail,
    ThreadListFilter, UpdateThreadRequest, UsageAggregation, UsageGroupBy, AgentRebindHint,
    CURRENT_RUNTIME_SCHEMA_VERSION, EVENT_CHANNEL_CAPACITY, RUNTIME_RESTART_REASON,
    SUMMARY_LIMIT,
};

fn load_routing_rules(path: &Path) -> Result<Vec<RoutingRule>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(path)?;
    let doc: RoutingRulesDoc = serde_json::from_str(&data)?;
    Ok(doc.rules)
}

fn save_routing_rules(path: &Path, rules: &[RoutingRule]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let doc = RoutingRulesDoc {
        rules: rules.to_vec(),
    };
    let json = serde_json::to_string_pretty(&doc)?;
    std::fs::write(path, json)?;
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveTurnState {
    pub(crate) turn_id: String,
    pub(crate) interrupt_requested: bool,
    pub(crate) auto_approve: bool,
    pub(crate) trust_mode: bool,
}

#[derive(Clone)]
pub(crate) struct ActiveThreadState {
    pub(crate) engine: EngineHandle,
    pub(crate) active_turn: Option<ActiveTurnState>,
}

#[derive(Default)]
pub(crate) struct ActiveThreads {
    pub(crate) engines: HashMap<String, ActiveThreadState>,
    pub(crate) lru: VecDeque<String>,
    pending_approvals: HashMap<String, PendingApproval>,
}

#[allow(dead_code)]
struct PendingApproval {
    thread_id: String,
    turn_id: String,
    tool_call_id: String,
    deadline: tokio::time::Instant,
}

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
    config: Config,
    workspace: PathBuf,
    pub(crate) store: RuntimeThreadStore,
    pub(crate) active: Arc<Mutex<ActiveThreads>>,
    event_tx: broadcast::Sender<RuntimeEventRecord>,
    manager_cfg: RuntimeThreadManagerConfig,
    cancel_token: CancellationToken,
    task_manager: Arc<StdMutex<Option<crate::task_manager::SharedTaskManager>>>,
    automations: Arc<StdMutex<Option<crate::automation_manager::SharedAutomationManager>>>,
    routing_rules: Arc<Mutex<Vec<RoutingRule>>>,
    routing_rules_path: PathBuf,
    checklist_cache: Arc<StdMutex<HashMap<String, String>>>,
    scratchpad_status_cache: Arc<StdMutex<HashMap<String, ScratchpadStatusCacheEntry>>>,
}

#[derive(Clone)]
struct ScratchpadStatusCacheEntry {
    fetched_at: Instant,
    status: Option<serde_json::Value>,
}

const SCRATCHPAD_STATUS_CACHE_TTL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeApprovalDecision {
    ApproveTool,
    DenyTool,
    RetryWithFullAccess,
}

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
            let _ = self.store.save_thread(&thread);
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

    /// Return the cached checklist snapshot for a thread (for DS Pick WebView panel).
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

    fn persist_thread_checklist(&self, thread_id: &str, checklist_json: &str) {
        if let Ok(mut cache) = self.checklist_cache.lock() {
            cache.insert(thread_id.to_string(), checklist_json.to_string());
        }
        let snapshot: Option<serde_json::Value> = serde_json::from_str(checklist_json).ok();
        if let Ok(mut thread) = self.store.load_thread(thread_id) {
            thread.checklist_snapshot = snapshot;
            thread.updated_at = Utc::now();
            if self.store.save_thread(&thread).is_err() {
                tracing::warn!(thread_id, "failed to persist checklist snapshot on thread");
            }
        }
    }

    /// DS Pick panel channel (C): push checklist snapshot on the live SSE stream (B-channel fallback).
    async fn emit_panel_checklist(&self, thread_id: &str, turn_id: &str) -> Result<()> {
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

    /// DS Pick panel channel (C): push audit scratchpad status on SSE.
    async fn emit_panel_scratchpad(&self, thread_id: &str, turn_id: &str) -> Result<()> {
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

    /// DS Pick panel channel (C): push context usage snapshot on SSE.
    async fn emit_panel_context(&self, thread_id: &str, turn_id: &str) -> Result<()> {
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

    fn scratchpad_tool_needs_panel_push(name: &str) -> bool {
        name.starts_with("scratchpad_")
    }

    fn checklist_tool_needs_panel_push(name: &str) -> bool {
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

    async fn emit_event(
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

    pub async fn create_thread(&self, req: CreateThreadRequest) -> Result<ThreadRecord> {
        let now = Utc::now();
        let model = req
            .model
            .filter(|m| !m.trim().is_empty())
            .or_else(|| self.config.default_text_model.clone())
            .unwrap_or_else(|| DEFAULT_TEXT_MODEL.to_string());
        let workspace = req.workspace.unwrap_or_else(|| self.workspace.clone());
        let mode = req
            .mode
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| "agent".to_string());
        let allow_shell = req.allow_shell.unwrap_or_else(|| self.config.allow_shell());
        let trust_mode = req.trust_mode.unwrap_or(false);
        let auto_approve = req.auto_approve.unwrap_or(false);
        let task_type = crate::task_type::resolve_task_type(req.task_type.as_deref(), &workspace, None);

        let thread = ThreadRecord {
            schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
            id: format!("thr_{}", &Uuid::new_v4().to_string()[..8]),
            created_at: now,
            updated_at: now,
            model,
            workspace,
            mode,
            allow_shell,
            trust_mode,
            auto_approve,
            latest_turn_id: None,
            latest_response_bookmark: None,
            archived: req.archived,
            system_prompt: req.system_prompt,
            task_id: req.task_id,
            title: None,
            task_type: task_type.as_str().to_string(),
            coherence_state: CoherenceState::default(),
            scratchpad_run_id: None,
            checklist_snapshot: None,
        };
        {
            let store = self.store.clone();
            let thread_clone = thread.clone();
            tokio::task::spawn_blocking(move || store.save_thread(&thread_clone))
                .await
                .map_err(|e| anyhow!("save thread panicked: {e}"))??;
        }
        self.emit_event(
            &thread.id,
            None,
            None,
            "thread.started",
            json!({ "thread": thread.clone() }),
        )
        .await?;
        Ok(thread)
    }

    pub async fn list_threads(
        &self,
        filter: ThreadListFilter,
        limit: Option<usize>,
    ) -> Result<Vec<ThreadRecord>> {
        let mut threads = self.store.list_threads()?;
        match filter {
            ThreadListFilter::ActiveOnly => threads.retain(|t| !t.archived),
            ThreadListFilter::ArchivedOnly => threads.retain(|t| t.archived),
            ThreadListFilter::IncludeArchived => {}
        }
        if let Some(limit) = limit {
            threads.truncate(limit);
        }
        Ok(threads)
    }

    /// Aggregate token + cost usage across all threads/turns inside the time
    /// range `[since, until]`. Each turn's cost is computed via
    /// `pricing::calculate_turn_cost_from_usage` using the *thread*'s model
    /// (turns inherit it). Whalescale#261 / #564.
    ///
    /// Buckets are sorted by ascending key for deterministic output. Empty
    /// ranges produce empty `buckets` (never an error).
    pub async fn aggregate_usage(
        &self,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
        group_by: UsageGroupBy,
    ) -> Result<UsageAggregation> {
        let threads = self.store.list_threads()?;
        let thread_models: HashMap<String, String> =
            threads.into_iter().map(|t| (t.id, t.model)).collect();
        self.store
            .aggregate_usage_linear(&thread_models, since, until, group_by)
    }

    pub async fn get_routing_rules(&self) -> Vec<RoutingRule> {
        self.routing_rules.lock().await.clone()
    }

    pub async fn set_routing_rules(&self, rules: Vec<RoutingRule>) -> Result<()> {
        *self.routing_rules.lock().await = rules.clone();
        save_routing_rules(&self.routing_rules_path, &rules)
    }

    pub async fn get_thread(&self, id: &str) -> Result<ThreadRecord> {
        self.load_thread_sync(id)
    }

    /// Sync version of `get_thread` callable from `spawn_blocking` context.
    pub fn load_thread_sync(&self, id: &str) -> Result<ThreadRecord> {
        self.store
            .load_thread(id)
            .with_context(|| format!("Thread not found: {id}"))
    }

    pub async fn update_thread(&self, id: &str, req: UpdateThreadRequest) -> Result<ThreadRecord> {
        if req.archived.is_none()
            && req.allow_shell.is_none()
            && req.trust_mode.is_none()
            && req.auto_approve.is_none()
            && req.model.is_none()
            && req.mode.is_none()
            && req.title.is_none()
            && req.system_prompt.is_none()
            && req.workspace.is_none()
            && req.scratchpad_run_id.is_none()
        {
            bail!("At least one thread field is required");
        }

        if let Some(model) = req.model.as_ref()
            && model.trim().is_empty()
        {
            bail!("model must not be empty");
        }
        if let Some(mode) = req.mode.as_ref()
            && mode.trim().is_empty()
        {
            bail!("mode must not be empty");
        }

        let mut thread = self.get_thread(id).await?;
        let mut changes = serde_json::Map::new();
        let mut eviction_needed = false;

        if let Some(archived) = req.archived
            && thread.archived != archived
        {
            thread.archived = archived;
            changes.insert("archived".to_string(), json!(archived));
        }
        if let Some(allow_shell) = req.allow_shell
            && thread.allow_shell != allow_shell
        {
            thread.allow_shell = allow_shell;
            changes.insert("allow_shell".to_string(), json!(allow_shell));
        }
        if let Some(trust_mode) = req.trust_mode
            && thread.trust_mode != trust_mode
        {
            thread.trust_mode = trust_mode;
            changes.insert("trust_mode".to_string(), json!(trust_mode));
        }
        if let Some(auto_approve) = req.auto_approve
            && thread.auto_approve != auto_approve
        {
            thread.auto_approve = auto_approve;
            changes.insert("auto_approve".to_string(), json!(auto_approve));
        }
        if let Some(model) = req.model
            && thread.model != model
        {
            thread.model = model.clone();
            changes.insert("model".to_string(), json!(model));
        }
        if let Some(mode) = req.mode
            && thread.mode != mode
        {
            thread.mode = mode.clone();
            changes.insert("mode".to_string(), json!(mode));
        }
        if let Some(title) = req.title {
            // Empty string clears a previously-set title and reverts to derived.
            let new_title = if title.trim().is_empty() {
                None
            } else {
                Some(title)
            };
            if thread.title != new_title {
                thread.title = new_title.clone();
                changes.insert("title".to_string(), json!(new_title));
            }
        }
        if let Some(system_prompt) = req.system_prompt {
            let new_sys = if system_prompt.trim().is_empty() {
                None
            } else {
                Some(system_prompt)
            };
            if thread.system_prompt != new_sys {
                thread.system_prompt = new_sys.clone();
                changes.insert("system_prompt".to_string(), json!(new_sys));
            }
        }
        if let Some(scratchpad_run_id) = req.scratchpad_run_id {
            let new_id = if scratchpad_run_id.trim().is_empty() {
                None
            } else {
                Some(scratchpad_run_id)
            };
            if thread.scratchpad_run_id != new_id {
                thread.scratchpad_run_id = new_id.clone();
                changes.insert("scratchpad_run_id".to_string(), json!(new_id));
            }
        }
        if let Some(workspace_raw) = req.workspace.clone() {
            let new_ws = Self::resolve_thread_workspace_path(&self.workspace, &workspace_raw)?;
            let old_canonical =
                fs::canonicalize(&thread.workspace).unwrap_or_else(|_| thread.workspace.clone());
            if new_ws != old_canonical {
                thread.workspace = new_ws;
                // Trigger a background symbol index rebuild so the first
                // grep_files call on the new workspace can use it immediately.
                let rebuild_ws = thread.workspace.clone();
                tokio::task::spawn_blocking(move || {
                    crate::symbol_index::ensure_symbol_index(&rebuild_ws);
                });
                eviction_needed = true;
                changes.insert(
                    "workspace".to_string(),
                    json!(thread.workspace.display().to_string()),
                );
            }
        }

        if !changes.is_empty() {
            thread.updated_at = Utc::now();
            {
                let store = self.store.clone();
                let thread_clone = thread.clone();
                tokio::task::spawn_blocking(move || store.save_thread(&thread_clone))
                    .await
                    .map_err(|e| anyhow!("save thread panicked: {e}"))??;
            }
            self.emit_event(
                &thread.id,
                None,
                None,
                "thread.updated",
                json!({
                    "thread": thread.clone(),
                    "changes": Value::Object(changes),
                }),
            )
            .await?;
        }

        if eviction_needed {
            self.unload_idle_thread_engine(id).await?;
        }

        Ok(thread)
    }

    pub async fn get_thread_detail(&self, id: &str) -> Result<ThreadDetail> {
        let thread = self.get_thread(id).await?;
        let turns = self.store.list_turns_for_thread(id)?;
        let mut items = Vec::new();
        for turn in &turns {
            items.extend(self.store.list_items_for_turn(&turn.id)?);
        }
        let latest_seq = self.store.current_seq().await;
        Ok(ThreadDetail {
            thread,
            turns,
            items,
            latest_seq,
        })
    }

    /// TUI-aligned context usage + compaction policy for DS Pick.
    pub async fn get_thread_context(&self, id: &str) -> Result<ThreadContextSnapshot> {
        let thread = self.get_thread(id).await?;
        let compaction = self.config.compaction_runtime_config(&thread.model);
        let system = thread
            .system_prompt
            .as_ref()
            .map(|s| SystemPrompt::Text(s.clone()));

        let last_turn = self
            .store
            .list_turns_for_thread(id)
            .ok()
            .and_then(|turns| turns.last().cloned());
        let last_api = last_turn
            .as_ref()
            .and_then(|t| t.last_request_input_tokens);
        let last_reported = last_turn
            .and_then(|t| t.usage)
            .map(|u| u.input_tokens);

        {
            let active = self.active.lock().await;
            if let Some(state) = active.engines.get(id) {
                let engine = state.engine.clone();
                drop(active);
                if let Ok(mut snapshot) = engine.query_context_snapshot().await {
                    if snapshot.last_reported_input_tokens.is_none() {
                        snapshot.last_reported_input_tokens = last_reported;
                    }
                    return Ok(snapshot);
                }
            }
        }

        let store = self.store.clone();
        let thread_id = id.to_string();
        let messages = tokio::task::spawn_blocking(move || -> Result<Vec<Message>> {
            let turns = store.list_turns_for_thread(&thread_id)?;
            reconstruct_messages_for_store(&store, &turns)
        })
        .await
        .map_err(|e| anyhow!("get_thread_context panicked: {e}"))??;

        Ok(build_thread_context_snapshot(
            &thread.model,
            &messages,
            system.as_ref(),
            &compaction,
            Some(&thread.workspace),
            last_api,
            last_reported,
            "store",
        ))
    }

    pub async fn resume_thread(&self, id: &str) -> Result<ThreadRecord> {
        let thread = self.get_thread(id).await?;
        self.ensure_engine_loaded(&thread).await?;
        Ok(thread)
    }

    /// Resume a thread and recover the sub-agent rebind hints needed to
    /// reconstruct in-transcript cards (issue #128). Drains the persisted
    /// `agent.*` event stream and collapses it into the latest known
    /// status per `agent_id` — the UI consumes this to seed empty
    /// `DelegateCard` / `FanoutCard` placeholders so subsequent live
    /// mailbox envelopes mutate them in place.
    #[allow(dead_code)] // exposed for the runtime API resume flow; consumed by #128 follow-up.
    pub async fn resume_thread_with_agent_rebind(
        &self,
        id: &str,
    ) -> Result<(ThreadRecord, Vec<AgentRebindHint>)> {
        let thread = self.resume_thread(id).await?;
        let events = self.store.events_since(&thread.id, None)?;
        let hints = collect_agent_rebind_hints(&events);
        Ok((thread, hints))
    }

    pub async fn fork_thread(&self, id: &str) -> Result<ThreadRecord> {
        let source = self.get_thread(id).await?;
        let mut forked = source.clone();
        let now = Utc::now();
        forked.id = format!("thr_{}", &Uuid::new_v4().to_string()[..8]);
        forked.created_at = now;
        forked.updated_at = now;
        forked.latest_turn_id = None;
        forked.archived = false;
        self.store.save_thread(&forked)?;

        let source_turns = self.store.list_turns_for_thread(&source.id)?;
        for source_turn in source_turns {
            let mut cloned_turn = source_turn.clone();
            cloned_turn.id = format!("turn_{}", &Uuid::new_v4().to_string()[..8]);
            cloned_turn.thread_id = forked.id.clone();
            cloned_turn.item_ids.clear();
            self.store.save_turn(&cloned_turn)?;

            let items = self.store.list_items_for_turn(&source_turn.id)?;
            for item in items {
                let mut cloned_item = item.clone();
                cloned_item.id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
                cloned_item.turn_id = cloned_turn.id.clone();
                self.store.save_item(&cloned_item)?;
                cloned_turn.item_ids.push(cloned_item.id.clone());
            }
            self.store.save_turn(&cloned_turn)?;
            forked.latest_turn_id = Some(cloned_turn.id.clone());
            forked.updated_at = now;
            self.store.save_thread(&forked)?;
        }

        self.emit_event(
            &forked.id,
            None,
            None,
            "thread.forked",
            json!({
                "thread": forked,
                "source_thread_id": source.id,
            }),
        )
        .await?;
        Ok(forked)
    }

    /// Fork a thread, dropping every turn from the Nth-from-tail user
    /// message onward (issue #133 — Esc-Esc backtrack).
    ///
    /// `depth_from_tail` selects which user turn to roll back *to*:
    ///
    /// - `0` — drop the most recent turn (the freshest user message and
    ///   everything after it)
    /// - `1` — drop the two most recent turns (rewind one further)
    /// - …and so on
    ///
    /// Returns a tuple of `(forked_thread, original_user_text)` where the
    /// second element is the `detail` of the first `UserMessage` item in
    /// the *first dropped* turn — i.e. the input the user typed to start
    /// that turn — so the caller can pre-populate the composer with it.
    /// `None` when no detail was recorded (defensive — every persisted
    /// `UserMessage` since v0.6 carries a detail string).
    ///
    /// Counts user turns by iterating `list_turns_for_thread` (sorted
    /// oldest → newest) backwards. A turn is counted as a "user turn"
    /// when at least one of its items has `kind ==
    /// TurnItemKind::UserMessage`. Steered turns (which append additional
    /// `UserMessage` items) still count as one turn — backtrack rewinds
    /// at the turn boundary, not at the steer boundary.
    ///
    /// Errors:
    /// - `depth_from_tail` exceeds the number of user turns
    /// - source thread not found
    #[allow(dead_code)] // exposed for the runtime/HTTP fork-on-backtrack path; the in-TUI Esc-Esc flow trims `App` state directly. Issue #133.
    pub async fn fork_at_user_message(
        &self,
        id: &str,
        depth_from_tail: usize,
    ) -> Result<(ThreadRecord, Option<String>)> {
        let source = self.get_thread(id).await?;
        let source_turns = self.store.list_turns_for_thread(&source.id)?;

        // Walk turns from newest to oldest. For each turn, ask: does it
        // contain a UserMessage item? If yes, it counts toward the depth.
        let mut user_turn_indices: Vec<usize> = Vec::new();
        for (idx, turn) in source_turns.iter().enumerate().rev() {
            let items = self.store.list_items_for_turn(&turn.id)?;
            if items
                .iter()
                .any(|item| item.kind == TurnItemKind::UserMessage)
            {
                user_turn_indices.push(idx);
            }
        }
        if depth_from_tail >= user_turn_indices.len() {
            bail!(
                "fork_at_user_message: depth {} exceeds {} user turn(s)",
                depth_from_tail,
                user_turn_indices.len()
            );
        }
        // `user_turn_indices` is newest-first because we iterated in
        // reverse, so the Nth element is exactly the Nth-from-tail user
        // turn in the original chronological list.
        let target_turn_idx = user_turn_indices[depth_from_tail];
        let target_turn_id = source_turns[target_turn_idx].id.clone();

        // Pull the original user-message text out of the dropped turn so
        // the caller can drop it back into the composer.
        let target_items = self.store.list_items_for_turn(&target_turn_id)?;
        let original_user_text = target_items
            .iter()
            .find(|item| item.kind == TurnItemKind::UserMessage)
            .and_then(|item| item.detail.clone());

        // Copy turns strictly before `target_turn_idx` into a new thread.
        // Mirrors `fork_thread` but stops at the cutoff instead of copying
        // every turn. Kept structurally close so future parity reviews
        // can spot drift between the two paths.
        let mut forked = source.clone();
        let now = Utc::now();
        forked.id = format!("thr_{}", &Uuid::new_v4().to_string()[..8]);
        forked.created_at = now;
        forked.updated_at = now;
        forked.latest_turn_id = None;
        forked.archived = false;
        self.store.save_thread(&forked)?;

        for source_turn in source_turns.iter().take(target_turn_idx) {
            let mut cloned_turn = source_turn.clone();
            cloned_turn.id = format!("turn_{}", &Uuid::new_v4().to_string()[..8]);
            cloned_turn.thread_id = forked.id.clone();
            cloned_turn.item_ids.clear();
            self.store.save_turn(&cloned_turn)?;

            let items = self.store.list_items_for_turn(&source_turn.id)?;
            for item in items {
                let mut cloned_item = item.clone();
                cloned_item.id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
                cloned_item.turn_id = cloned_turn.id.clone();
                self.store.save_item(&cloned_item)?;
                cloned_turn.item_ids.push(cloned_item.id.clone());
            }
            self.store.save_turn(&cloned_turn)?;
            forked.latest_turn_id = Some(cloned_turn.id.clone());
            forked.updated_at = now;
            self.store.save_thread(&forked)?;
        }

        self.emit_event(
            &forked.id,
            None,
            None,
            "thread.forked",
            json!({
                "thread": forked,
                "source_thread_id": source.id,
                "backtrack_depth_from_tail": depth_from_tail,
                "dropped_turn_id": target_turn_id,
            }),
        )
        .await?;
        Ok((forked, original_user_text))
    }

    /// Seed a thread with messages from a saved session so subsequent turns
    /// continue with the prior conversation context.
    pub async fn seed_thread_from_messages(
        &self,
        thread_id: &str,
        messages: &[Message],
    ) -> Result<()> {
        let mut thread = self.get_thread(thread_id).await?;
        let now = Utc::now();

        let mut user_buf: Vec<String> = Vec::new();
        let mut pending_pairs: Vec<(String, Option<String>)> = Vec::new();

        for msg in messages {
            let text = msg
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if text.trim().is_empty() {
                continue;
            }
            if msg.role == "user" {
                user_buf.push(text);
            } else if msg.role == "assistant" {
                let user_text = if user_buf.is_empty() {
                    String::new()
                } else {
                    std::mem::take(&mut user_buf).join("\n")
                };
                pending_pairs.push((user_text, Some(text)));
            }
        }
        if !user_buf.is_empty() {
            let user_text = std::mem::take(&mut user_buf).join("\n");
            pending_pairs.push((user_text, None));
        }

        for (user_text, assistant_text) in pending_pairs {
            let turn_id = format!("turn_{}", &Uuid::new_v4().to_string()[..8]);
            let summary = crate::utils::truncate_with_ellipsis(&user_text, SUMMARY_LIMIT, "...");
            let mut item_ids = Vec::new();

            if !user_text.is_empty() {
                let item_id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
                self.store.save_item(&TurnItemRecord {
                    schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                    id: item_id.clone(),
                    turn_id: turn_id.clone(),
                    kind: TurnItemKind::UserMessage,
                    status: TurnItemLifecycleStatus::Completed,
                    summary: summary.clone(),
                    detail: Some(user_text),
                    metadata: None,
                    artifact_refs: Vec::new(),
                    started_at: Some(now),
                    ended_at: Some(now),
                })?;
                item_ids.push(item_id);
            }

            if let Some(assistant_text) = assistant_text {
                let asst_summary =
                    crate::utils::truncate_with_ellipsis(&assistant_text, SUMMARY_LIMIT, "...");
                let item_id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
                self.store.save_item(&TurnItemRecord {
                    schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                    id: item_id.clone(),
                    turn_id: turn_id.clone(),
                    kind: TurnItemKind::AgentMessage,
                    status: TurnItemLifecycleStatus::Completed,
                    summary: asst_summary,
                    detail: Some(assistant_text),
                    metadata: None,
                    artifact_refs: Vec::new(),
                    started_at: Some(now),
                    ended_at: Some(now),
                })?;
                item_ids.push(item_id);
            }

            self.store.save_turn(&TurnRecord {
                schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                id: turn_id.clone(),
                thread_id: thread_id.to_string(),
                status: RuntimeTurnStatus::Completed,
                input_summary: summary,
                created_at: now,
                started_at: Some(now),
                ended_at: Some(now),
                duration_ms: Some(0),
                usage: None,
                last_request_input_tokens: None,
                error: None,
                item_ids,
                steer_count: 0,
            })?;

            thread.latest_turn_id = Some(turn_id);
            thread.updated_at = now;
        }

        self.store.save_thread(&thread)?;
        self.emit_event(
            thread_id,
            None,
            None,
            "thread.updated",
            json!({ "thread": thread, "reason": "session_resume" }),
        )
        .await?;
        Ok(())
    }

    pub async fn start_turn(&self, thread_id: &str, req: StartTurnRequest) -> Result<TurnRecord> {
        let prompt = req.prompt.trim().to_string();
        if prompt.is_empty() {
            bail!("prompt is required");
        }

        // —— Model routing ———
        let mut req = req;
        if let Some(ref intent) = req.route_intent {
            let rules = self.routing_rules.lock().await;
            if let Some(rule) = rules.iter().find(|r| r.intent.eq_ignore_ascii_case(intent)) {
                req.model = Some(rule.model.clone());
            }
        }

        let mut thread = self.get_thread(thread_id).await?;
        let engine = self.ensure_engine_loaded(&thread).await?;

        {
            let active = self.active.lock().await;
            if let Some(active_thread) = active.engines.get(thread_id)
                && active_thread.active_turn.is_some()
            {
                bail!("Thread already has an active turn");
            }
        }

        let now = Utc::now();
        let turn_id = format!("turn_{}", &Uuid::new_v4().to_string()[..8]);
        let mut turn = TurnRecord {
            schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
            id: turn_id.clone(),
            thread_id: thread_id.to_string(),
            status: RuntimeTurnStatus::InProgress,
            input_summary: req
                .input_summary
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
            schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
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
            let store = self.store.clone();
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

        self.emit_event(
            thread_id,
            Some(&turn_id),
            None,
            "turn.started",
            json!({ "turn": turn.clone() }),
        )
        .await?;
        self.emit_event(
            thread_id,
            Some(&turn_id),
            Some(&user_item_id),
            "item.started",
            json!({ "item": user_item.clone() }),
        )
        .await?;
        self.emit_event(
            thread_id,
            Some(&turn_id),
            Some(&user_item_id),
            "item.completed",
            json!({ "item": user_item }),
        )
        .await?;

        {
            let mut active = self.active.lock().await;
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

        let mode = parse_mode(req.mode.as_deref().unwrap_or(&thread.mode));
        let requested_model = req.model.unwrap_or_else(|| thread.model.clone());
        let auto_model = requested_model.trim().eq_ignore_ascii_case("auto");
        let (model, reasoning_effort) = if auto_model {
            let selection = crate::commands::resolve_auto_route_with_flash(
                &self.config,
                &prompt,
                "",
                "auto",
                "auto",
            )
            .await;
            (
                selection.model,
                selection
                    .reasoning_effort
                    .map(|effort| effort.as_setting().to_string()),
            )
        } else {
            (requested_model, None)
        };
        let allow_shell = req.allow_shell.unwrap_or(thread.allow_shell);
        let trust_mode = req.trust_mode.unwrap_or(thread.trust_mode);
        let auto_approve = req.auto_approve.unwrap_or(thread.auto_approve);

        engine
            .send(Op::SendMessage {
                content: prompt,
                mode,
                model: model.clone(),
                goal_objective: None,
                reasoning_effort,
                reasoning_effort_auto: auto_model,
                auto_model,
                allow_shell,
                trust_mode,
                auto_approve,
                approval_mode: if auto_approve {
                    crate::tui::approval::ApprovalMode::Auto
                } else {
                    crate::tui::approval::ApprovalMode::Suggest
                },
            })
            .await
            .map_err(|e| anyhow!("Failed to start turn: {e}"))?;

        let manager = Arc::new(self.clone());
        let thread_id_owned = thread_id.to_string();
        let turn_id_owned = turn_id.clone();
        let engine_clone = engine.clone();
        let cancel_token = self.cancel_token.clone();
        tokio::spawn(async move {
            if cancel_token.is_cancelled() {
                tracing::debug!("Skipping turn monitor: shutdown requested");
                return;
            }
            use futures_util::FutureExt;
            let result = std::panic::AssertUnwindSafe(manager.monitor_turn(
                thread_id_owned,
                turn_id_owned,
                engine_clone,
            ))
            .catch_unwind()
            .await;
            match result {
                Ok(res) => {
                    if let Err(err) = res {
                        tracing::error!("Failed to monitor turn: {err}");
                    }
                }
                Err(panic_err) => {
                    if let Some(msg) = panic_err.downcast_ref::<&str>() {
                        tracing::error!("Turn monitor panicked: {}", msg);
                    } else if let Some(msg) = panic_err.downcast_ref::<String>() {
                        tracing::error!("Turn monitor panicked: {}", msg);
                    } else {
                        tracing::error!("Turn monitor panicked with unknown error");
                    }
                }
            }
        });

        Ok(turn)
    }

    pub async fn interrupt_turn(&self, thread_id: &str, turn_id: &str) -> Result<TurnRecord> {
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
            schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
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
        {
            let store = self.store.clone();
            let turn_clone = turn.clone();
            let item_clone = item.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
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

    pub async fn compact_thread(
        &self,
        thread_id: &str,
        req: CompactThreadRequest,
    ) -> Result<TurnRecord> {
        let mut thread = self.get_thread(thread_id).await?;
        let engine = self.ensure_engine_loaded(&thread).await?;

        {
            let active = self.active.lock().await;
            if let Some(active_thread) = active.engines.get(thread_id)
                && active_thread.active_turn.is_some()
            {
                bail!("Thread already has an active turn");
            }
        }

        let now = Utc::now();
        let turn_id = format!("turn_{}", &Uuid::new_v4().to_string()[..8]);
        let turn = TurnRecord {
            schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
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
            let store = self.store.clone();
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
            let mut active = self.active.lock().await;
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

        self.emit_event(
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

        let manager = Arc::new(self.clone());
        let thread_id_owned = thread_id.to_string();
        let turn_id_owned = turn_id.clone();
        let engine_clone = engine.clone();
        let cancel_token = self.cancel_token.clone();
        tokio::spawn(async move {
            if cancel_token.is_cancelled() {
                tracing::debug!("Skipping compaction monitor: shutdown requested");
                return;
            }
            use futures_util::FutureExt;
            let result = std::panic::AssertUnwindSafe(manager.monitor_turn(
                thread_id_owned,
                turn_id_owned,
                engine_clone,
            ))
            .catch_unwind()
            .await;
            match result {
                Ok(res) => {
                    if let Err(err) = res {
                        tracing::error!("Failed to monitor compaction turn: {err}");
                    }
                }
                Err(panic_err) => {
                    if let Some(msg) = panic_err.downcast_ref::<&str>() {
                        tracing::error!("Compaction monitor panicked: {}", msg);
                    } else if let Some(msg) = panic_err.downcast_ref::<String>() {
                        tracing::error!("Compaction monitor panicked: {}", msg);
                    } else {
                        tracing::error!("Compaction monitor panicked with unknown error");
                    }
                }
            }
        });

        Ok(turn)
    }

    pub fn events_since(
        &self,
        thread_id: &str,
        since_seq: Option<u64>,
    ) -> Result<Vec<RuntimeEventRecord>> {
        self.store.events_since(thread_id, since_seq)
    }

    async fn ensure_engine_loaded(&self, thread: &ThreadRecord) -> Result<EngineHandle> {
        {
            let mut active = self.active.lock().await;
            if let Some(engine) = active
                .engines
                .get(thread.id.as_str())
                .map(|state| state.engine.clone())
            {
                touch_lru(&mut active.lru, &thread.id);
                return Ok(engine);
            }
        }

        // Compaction defaults from config.toml `[compaction]` (DS Pick system
        // settings) with model-derived threshold fallback.
        let compaction = self.config.compaction_runtime_config(&thread.model);
        let network_policy = self.config.network.clone().map(|toml_cfg| {
            crate::network_policy::NetworkPolicyDecider::with_default_audit(toml_cfg.into_runtime())
        });
        let lsp_config = self
            .config
            .lsp
            .clone()
            .map(crate::config::LspConfigToml::into_runtime);
        let scratchpad_run_id_slot = std::sync::Arc::new(std::sync::Mutex::new(
            thread.scratchpad_run_id.clone(),
        ));
        let store = self.store.clone();
        let thread_id_persist = thread.id.clone();
        let persist_scratchpad: std::sync::Arc<dyn Fn(String) + Send + Sync> =
            std::sync::Arc::new(move |run_id: String| {
                if let Ok(mut t) = store.load_thread(&thread_id_persist) {
                    if t.scratchpad_run_id.as_deref() != Some(run_id.as_str()) {
                        t.scratchpad_run_id = Some(run_id);
                        t.updated_at = Utc::now();
                        let _ = store.save_thread(&t);
                    }
                }
            });
        let engine_cfg = EngineConfig {
            model: thread.model.clone(),
            workspace: thread.workspace.clone(),
            allow_shell: thread.allow_shell,
            trust_mode: thread.trust_mode,
            notes_path: self.config.notes_path(),
            mcp_config_path: self.config.mcp_config_path(),
            skills_dir: self.config.skills_dir(),
            instructions: crate::prompts::merge_instruction_paths_with_pick_rules(
                &thread.workspace,
                self.config.instructions_paths(),
            ),
            max_steps: 100,
            max_subagents: self.config.max_subagents().clamp(1, MAX_SUBAGENTS),
            subagent_step_timeout: self.config.subagent_step_timeout(),
            features: self.config.features(),
            compaction,
            cycle: crate::cycle_manager::CycleConfig::default(),
            capacity: crate::core::capacity::capacity_config_from_app(
                &self.config,
            ),
            todos: new_shared_todo_list(),
            plan_state: new_shared_plan_state(),
            max_spawn_depth: crate::tools::subagent::DEFAULT_MAX_SPAWN_DEPTH,
            network_policy,
            snapshots_enabled: self.config.snapshots_config().enabled,
            lsp_config,
            runtime_services: crate::tools::spec::RuntimeToolServices {
                task_manager: self.task_manager.lock().ok().and_then(|slot| slot.clone()),
                automations: self.automations.lock().ok().and_then(|slot| slot.clone()),
                task_data_dir: Some(self.manager_cfg.task_data_dir.clone()),
                active_task_id: thread.task_id.clone(),
                active_thread_id: Some(thread.id.clone()),
                shell_manager: None,
                hook_executor: None,
                scratchpad_run_id: scratchpad_run_id_slot,
                persist_scratchpad_run_id: Some(persist_scratchpad),
                scratchpad_config: Some(self.config.scratchpad_config()),
            },
            subagent_model_overrides: self.config.subagent_model_overrides(),
            memory_enabled: self.config.memory_enabled(),
            memory_path: self.config.memory_path(),
            strict_tool_mode: self.config.strict_tool_mode.unwrap_or(false),
            goal_objective: None,
            locale_tag: crate::localization::resolve_locale(
                &crate::settings::Settings::load().unwrap_or_default().locale,
            )
            .tag()
            .to_string(),
            task_type: crate::task_type::TaskType::parse_str(&thread.task_type)
                .unwrap_or(crate::task_type::TaskType::Code),
            workshop: self.config.workshop.clone(),
            scratchpad: self.config.scratchpad_config(),
        };

        let engine = spawn_engine(engine_cfg, &self.config);

        // list_turns_for_thread + list_items_for_turn scan the entire turns/ &
        // items/ directories — O(n) across all threads. Run them on the
        // blocking thread pool so they never block a tokio worker and starve
        // the /health endpoint (ERR_CONNECTION_REFUSED loop).
        let store = self.store.clone();
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

        let mut active = self.active.lock().await;
        let evicted = enforce_lru_capacity(&mut active, self.manager_cfg.max_active_threads);
        active.engines.insert(
            thread.id.clone(),
            ActiveThreadState {
                engine: engine.clone(),
                active_turn: None,
            },
        );
        touch_lru(&mut active.lru, &thread.id);
        drop(active);
        for handle in evicted {
            let _ = handle.send(Op::Shutdown).await;
        }
        Ok(engine)
    }

    fn resolve_thread_workspace_path(manager_workspace: &Path, raw: &str) -> Result<PathBuf> {
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

    async fn unload_idle_thread_engine(&self, thread_id: &str) -> Result<()> {
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

    async fn monitor_turn(
        &self,
        thread_id: String,
        turn_id: String,
        engine: EngineHandle,
    ) -> Result<()> {
        let mut current_message_item: Option<(String, String)> = None;
        // Synthetic item id for thinking/reasoning deltas (not persisted as a TurnItem).
        let mut thinking_stream_item_id: Option<String> = None;
        let mut tool_items: HashMap<String, String> = HashMap::new();
        let mut compaction_items: HashMap<String, String> = HashMap::new();
        let mut turn_usage: Option<Usage> = None;
        let mut turn_last_request_input_tokens: Option<u32> = None;
        let mut turn_status = RuntimeTurnStatus::Completed;
        let mut turn_error: Option<String> = None;
        let mut turn_summary: Option<serde_json::Value> = None;

        loop {
            let event = {
                let mut rx = engine.rx_event.write().await;
                rx.recv().await
            };
            let Some(event) = event else {
                if self
                    .is_interrupt_requested(&thread_id, &turn_id)
                    .await
                    .unwrap_or(false)
                {
                    turn_status = RuntimeTurnStatus::Interrupted;
                }
                break;
            };

            match event {
                EngineEvent::TurnStarted { .. } => {
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        None,
                        "turn.lifecycle",
                        json!({ "status": "in_progress" }),
                    )
                    .await?;
                }
                EngineEvent::ThinkingStarted { .. } => {
                    thinking_stream_item_id =
                        Some(format!("item_{}", &Uuid::new_v4().to_string()[..8]));
                }
                EngineEvent::ThinkingDelta { content, .. } => {
                    if let Some(ref item_id) = thinking_stream_item_id {
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(item_id.as_str()),
                            "item.delta",
                            json!({ "delta": content, "kind": "thinking" }),
                        )
                        .await?;
                    }
                }
                EngineEvent::ThinkingComplete { .. } => {
                    thinking_stream_item_id = None;
                }
                EngineEvent::MessageStarted { .. } => {
                    let item_id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: item_id.clone(),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::AgentMessage,
                        status: TurnItemLifecycleStatus::InProgress,
                        summary: String::new(),
                        detail: Some(String::new()),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: None,
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item_id),
                        "item.started",
                        json!({ "item": item }),
                    )
                    .await?;
                    current_message_item = Some((item_id, String::new()));
                }
                EngineEvent::MessageDelta { content, .. } => {
                    if let Some((item_id, text)) = current_message_item.as_mut() {
                        text.push_str(&content);
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(item_id),
                            "item.delta",
                            json!({ "delta": content, "kind": "agent_message" }),
                        )
                        .await?;
                    }
                }
                EngineEvent::MessageComplete { .. } => {
                    if let Some((item_id, text)) = current_message_item.take() {
                        let item = self
                            .update_and_save_item_blocking(&item_id, |item| {
                                item.status = TurnItemLifecycleStatus::Completed;
                                item.summary = summarize_text(&text, SUMMARY_LIMIT);
                                item.detail = Some(text);
                                item.ended_at = Some(Utc::now());
                            })
                            .await?;
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(&item_id),
                            "item.completed",
                            json!({ "item": item }),
                        )
                        .await?;
                    }
                    let mgr = self.clone();
                    let tid = thread_id.clone();
                    let tturn = turn_id.clone();
                    tokio::spawn(async move {
                        let _ = mgr.emit_panel_context(&tid, &tturn).await;
                    });
                }
                EngineEvent::ToolCallStarted { id, name, input } => {
                    let item_id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
                    tool_items.insert(id.clone(), item_id.clone());
                    let kind = tool_kind_for_name(&name);
                    let summary = summarize_text(&format!("{name} started"), SUMMARY_LIMIT);
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: item_id.clone(),
                        turn_id: turn_id.clone(),
                        kind,
                        status: TurnItemLifecycleStatus::InProgress,
                        summary,
                        detail: Some(serde_json::to_string(&input).unwrap_or_default()),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: None,
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item_id),
                        "item.started",
                        json!({ "item": item, "tool": { "id": id, "name": name, "input": input } }),
                    )
                    .await?;
                }
                EngineEvent::ToolCallProgress { id, output } => {
                    if let Some(item_id) = tool_items.get(&id) {
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(item_id),
                            "item.delta",
                            json!({ "delta": output, "kind": "tool_call" }),
                        )
                        .await?;
                    }
                }
                EngineEvent::ToolCallComplete { id, name, result } => {
                    if let Some(item_id) = tool_items.remove(&id) {
                        let item = self
                            .update_and_save_item_blocking(&item_id, |item| {
                                let now = Utc::now();
                                item.ended_at = Some(now);
                                match &result {
                                    Ok(output) => {
                                        item.status = if output.success {
                                            TurnItemLifecycleStatus::Completed
                                        } else {
                                            TurnItemLifecycleStatus::Failed
                                        };
                                        item.summary = summarize_text(
                                            &format!("{name}: {}", output.content),
                                            SUMMARY_LIMIT,
                                        );
                                        item.detail = Some(output.content.clone());
                                        item.metadata = output.metadata.clone();
                                    }
                                    Err(err) => {
                                        item.status = TurnItemLifecycleStatus::Failed;
                                        item.summary = summarize_text(
                                            &format!("{name} failed: {err}"),
                                            SUMMARY_LIMIT,
                                        );
                                        item.detail = Some(err.to_string());
                                    }
                                }
                            })
                            .await?;
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(&item_id),
                            if item.status == TurnItemLifecycleStatus::Completed {
                                "item.completed"
                            } else {
                                "item.failed"
                            },
                            // Include engine tool-call id so compat SSE (`tool.completed`) matches `tool.started`.
                            json!({ "item": item, "tool": { "id": id, "name": name } }),
                        )
                        .await?;

                        // Cache checklist snapshot for the WebView checklist panel
                        if matches!(
                            name.as_str(),
                            "checklist_write"
                                | "checklist_add"
                                | "checklist_update"
                                | "todo_write"
                                | "todo_add"
                                | "todo_update"
                        ) {
                            if let Ok(output) = &result {
                                if output.success {
                                    if let Some(meta) = &output.metadata {
                                        if let Some(task_updates) = meta.get("task_updates") {
                                            if let Some(checklist_json) =
                                                task_updates.get("checklist")
                                            {
                                                if let Ok(json_str) =
                                                    serde_json::to_string(checklist_json)
                                                {
                                                    self.persist_thread_checklist(
                                                        &thread_id,
                                                        &json_str,
                                                    );
                                                    let _ = self
                                                        .emit_panel_checklist(
                                                            &thread_id,
                                                            &turn_id,
                                                        )
                                                        .await;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if Self::checklist_tool_needs_panel_push(name.as_str()) {
                            let _ = self
                                .emit_panel_checklist(&thread_id, &turn_id)
                                .await;
                        }
                        if Self::scratchpad_tool_needs_panel_push(name.as_str()) {
                            if result.as_ref().is_ok_and(|o| o.success) {
                                let _ = self
                                    .emit_panel_scratchpad(&thread_id, &turn_id)
                                    .await;
                            }
                        }
                    }
                }
                EngineEvent::CompactionStarted { id, auto, message } => {
                    let item_id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
                    compaction_items.insert(id.clone(), item_id.clone());
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: item_id.clone(),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::ContextCompaction,
                        status: TurnItemLifecycleStatus::InProgress,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message.clone()),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: None,
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item_id),
                        "item.started",
                        json!({ "item": item, "auto": auto }),
                    )
                    .await?;
                }
                EngineEvent::CompactionCompleted {
                    id,
                    auto,
                    message,
                    messages_before,
                    messages_after,
                } => {
                    if let Some(item_id) = compaction_items.remove(&id) {
                        let item = self
                            .update_and_save_item_blocking(&item_id, |item| {
                                item.status = TurnItemLifecycleStatus::Completed;
                                item.summary = summarize_text(&message, SUMMARY_LIMIT);
                                item.detail = Some(message);
                                item.ended_at = Some(Utc::now());
                            })
                            .await?;
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(&item_id),
                            "item.completed",
                            json!({
                                "item": item,
                                "auto": auto,
                                "messages_before": messages_before,
                                "messages_after": messages_after,
                            }),
                        )
                        .await?;
                    }
                }
                EngineEvent::CompactionFailed { id, auto, message } => {
                    if let Some(item_id) = compaction_items.remove(&id) {
                        let item = self
                            .update_and_save_item_blocking(&item_id, |item| {
                                item.status = TurnItemLifecycleStatus::Failed;
                                item.summary = summarize_text(&message, SUMMARY_LIMIT);
                                item.detail = Some(message);
                                item.ended_at = Some(Utc::now());
                            })
                            .await?;
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(&item_id),
                            "item.failed",
                            json!({ "item": item, "auto": auto }),
                        )
                        .await?;
                    }
                }
                EngineEvent::CycleAdvanced { from, to, briefing } => {
                    // Surface the cycle boundary in the runtime event timeline so
                    // background-task subscribers and replay see it. The actual
                    // archive write is the engine's responsibility (see
                    // `cycle_manager::archive_cycle`); this event is informational.
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        None,
                        "cycle.advanced",
                        json!({
                            "from": from,
                            "to": to,
                            "briefing_tokens": briefing.token_estimate,
                            "cycle": briefing.cycle,
                            "timestamp": briefing.timestamp,
                        }),
                    )
                    .await?;
                }
                EngineEvent::CoherenceState {
                    state,
                    label,
                    description,
                    reason,
                } => {
                    let mut thread = self.store.load_thread(&thread_id)?;
                    thread.coherence_state = state;
                    thread.updated_at = Utc::now();
                    {
                        let store = self.store.clone();
                        let thread_clone = thread.clone();
                        tokio::task::spawn_blocking(move || store.save_thread(&thread_clone))
                            .await
                            .map_err(|e| anyhow!("save thread panicked: {e}"))??;
                    }
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        None,
                        "coherence.state",
                        json!({
                            "state": state,
                            "label": label,
                            "description": description,
                            "reason": reason,
                            "thread": thread,
                        }),
                    )
                    .await?;
                }
                EngineEvent::CapacityDecision {
                    risk_band,
                    action,
                    reason,
                    ..
                } => {
                    let message = format!(
                        "Capacity decision: risk={risk_band} action={action} reason={reason}"
                    );
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "item.completed",
                        json!({ "item": item }),
                    )
                    .await?;
                }
                EngineEvent::CapacityIntervention {
                    action,
                    before_prompt_tokens,
                    after_prompt_tokens,
                    replay_outcome,
                    replan_performed,
                    ..
                } => {
                    let message = format!(
                        "Capacity intervention: {action} (~{before_prompt_tokens} -> ~{after_prompt_tokens}) replay={:?} replan={replan_performed}",
                        replay_outcome
                    );
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "item.completed",
                        json!({ "item": item }),
                    )
                    .await?;
                }
                EngineEvent::CapacityMemoryPersistFailed { action, error, .. } => {
                    let message =
                        format!("Capacity memory persist failed: action={action} error={error}");
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Failed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "item.failed",
                        json!({ "item": item }),
                    )
                    .await?;
                }
                EngineEvent::AgentSpawned { id, prompt } => {
                    let message = format!(
                        "Sub-agent {id} spawned: {}",
                        summarize_text(&prompt, SUMMARY_LIMIT)
                    );
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "agent.spawned",
                        json!({ "item": item, "agent_id": id, "prompt": prompt }),
                    )
                    .await?;
                }
                EngineEvent::AgentProgress { id, status } => {
                    let message = format!("Sub-agent {id}: {status}");
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "agent.progress",
                        json!({ "item": item, "agent_id": id }),
                    )
                    .await?;
                }
                EngineEvent::AgentComplete { id, result } => {
                    let message = format!(
                        "Sub-agent {id} completed: {}",
                        summarize_text(&result, SUMMARY_LIMIT)
                    );
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "agent.completed",
                        json!({ "item": item, "agent_id": id }),
                    )
                    .await?;
                }
                EngineEvent::AgentList { agents } => {
                    let running = agents
                        .iter()
                        .filter(|agent| matches!(agent.status, SubAgentStatus::Running))
                        .count();
                    let interrupted = agents
                        .iter()
                        .filter(|agent| matches!(agent.status, SubAgentStatus::Interrupted(_)))
                        .count();
                    let completed = agents
                        .iter()
                        .filter(|agent| matches!(agent.status, SubAgentStatus::Completed))
                        .count();
                    let message = format!(
                        "Sub-agent list refreshed: {} total ({running} running, {interrupted} interrupted, {completed} completed)",
                        agents.len()
                    );
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "agent.list",
                        json!({ "item": item, "agents": agents }),
                    )
                    .await?;
                }
                EngineEvent::ApprovalRequired {
                    id,
                    tool_name,
                    description,
                    ..
                } => {
                    if self
                        .active_turn_flags(&thread_id, &turn_id)
                        .await
                        .is_none()
                    {
                        let _ = engine.deny_tool_call(id).await;
                        continue;
                    }
                    let (auto_approve, trust_mode) = self
                        .active_turn_flags(&thread_id, &turn_id)
                        .await
                        .unwrap_or((false, false));
                    match Self::approval_decision(auto_approve, trust_mode, false) {
                        RuntimeApprovalDecision::ApproveTool => {
                            let _ = engine.approve_tool_call(id).await;
                        }
                        RuntimeApprovalDecision::DenyTool
                        | RuntimeApprovalDecision::RetryWithFullAccess => {
                            self.emit_event(
                                &thread_id,
                                Some(&turn_id),
                                None,
                                "approval.required",
                                json!({
                                    "id": id,
                                    "tool_name": tool_name,
                                    "description": description,
                                }),
                            )
                            .await?;

                            // Register as pending — wait for HTTP approval
                            // instead of immediate deny. A spawned timeout guard
                            // will auto-deny after the configured interval.
                            let timeout_secs = self.manager_cfg.http_approval_timeout_secs.max(1);
                            let deadline = tokio::time::Instant::now()
                                + std::time::Duration::from_secs(timeout_secs);
                            {
                                let mut active = self.active.lock().await;
                                active.pending_approvals.insert(
                                    id.clone(),
                                    PendingApproval {
                                        thread_id: thread_id.clone(),
                                        turn_id: turn_id.clone(),
                                        tool_call_id: id.clone(),
                                        deadline,
                                    },
                                );
                            }

                            let this = self.clone();
                            let engine_handle = engine.clone();
                            let tool_id = id.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep_until(deadline).await;
                                let mut active = this.active.lock().await;
                                if active.pending_approvals.remove(&tool_id).is_some() {
                                    drop(active);
                                    let _ = engine_handle.deny_tool_call(tool_id).await;
                                }
                            });
                        }
                    }
                }
                EngineEvent::ElevationRequired {
                    tool_id,
                    tool_name,
                    denial_reason,
                    ..
                } => {
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        None,
                        "sandbox.denied",
                        json!({
                            "tool_id": tool_id,
                            "tool_name": tool_name,
                            "reason": denial_reason,
                        }),
                    )
                    .await?;
                    let (auto_approve, trust_mode) = self
                        .active_turn_flags(&thread_id, &turn_id)
                        .await
                        .unwrap_or((false, false));
                    match Self::approval_decision(auto_approve, trust_mode, true) {
                        RuntimeApprovalDecision::RetryWithFullAccess => {
                            let _ = engine
                                .retry_tool_with_policy(
                                    tool_id,
                                    crate::sandbox::SandboxPolicy::DangerFullAccess,
                                )
                                .await;
                        }
                        RuntimeApprovalDecision::ApproveTool
                        | RuntimeApprovalDecision::DenyTool => {
                            let _ = engine.deny_tool_call(tool_id).await;
                        }
                    }
                }
                EngineEvent::Status { message } => {
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message.clone()),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "item.completed",
                        json!({ "item": item }),
                    )
                    .await?;
                }
                EngineEvent::Error { envelope, .. } => {
                    turn_status = RuntimeTurnStatus::Failed;
                    turn_error = Some(envelope.message.clone());
                    let message = envelope.message.clone();
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Error,
                        status: TurnItemLifecycleStatus::Failed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "item.failed",
                        json!({ "item": item }),
                    )
                    .await?;
                }
                EngineEvent::TurnComplete {
                    usage,
                    last_request_input_tokens,
                    status,
                    error,
                    step_count,
                    tool_names,
                    end_reason,
                } => {
                    turn_usage = Some(usage);
                    turn_last_request_input_tokens = last_request_input_tokens;
                    turn_status = match status {
                        TurnOutcomeStatus::Completed => RuntimeTurnStatus::Completed,
                        TurnOutcomeStatus::Interrupted => RuntimeTurnStatus::Interrupted,
                        TurnOutcomeStatus::Failed => RuntimeTurnStatus::Failed,
                    };
                    if let Some(err) = error {
                        turn_error = Some(err);
                    }
                    turn_summary = Some(json!({
                        "step_count": step_count,
                        "tool_names": tool_names,
                        "end_reason": end_reason,
                    }));
                    let _ = self.emit_panel_context(&thread_id, &turn_id).await;
                    let _ = self.emit_panel_scratchpad(&thread_id, &turn_id).await;
                    let _ = self.emit_panel_checklist(&thread_id, &turn_id).await;
                    break;
                }
                _ => {}
            }
        }

        if self
            .is_interrupt_requested(&thread_id, &turn_id)
            .await
            .unwrap_or(false)
        {
            turn_status = RuntimeTurnStatus::Interrupted;
        }

        if let Some((item_id, text)) = current_message_item.take() {
            let mut item = self.store.load_item(&item_id)?;
            if turn_status == RuntimeTurnStatus::Interrupted {
                item.status = TurnItemLifecycleStatus::Interrupted;
            } else {
                item.status = TurnItemLifecycleStatus::Completed;
            }
            item.summary = summarize_text(&text, SUMMARY_LIMIT);
            item.detail = Some(text);
            item.ended_at = Some(Utc::now());
            {
                let store = self.store.clone();
                let item_clone = item.clone();
                tokio::task::spawn_blocking(move || store.save_item(&item_clone))
                    .await
                    .map_err(|e| anyhow!("save item panicked: {e}"))??;
            }
            self.emit_event(
                &thread_id,
                Some(&turn_id),
                Some(&item_id),
                if item.status == TurnItemLifecycleStatus::Interrupted {
                    "item.interrupted"
                } else {
                    "item.completed"
                },
                json!({ "item": item }),
            )
            .await?;
        }

        let ended_at = Utc::now();
        let mut turn = self.store.load_turn(&turn_id)?;
        turn.status = turn_status;
        turn.ended_at = Some(ended_at);
        turn.duration_ms = turn.started_at.map(|start| duration_ms(start, ended_at));
        turn.usage = turn_usage;
        turn.last_request_input_tokens = turn_last_request_input_tokens;
        turn.error = turn_error;

        let mut thread = self.get_thread(&thread_id).await?;
        thread.latest_turn_id = Some(turn_id.clone());
        thread.updated_at = Utc::now();

        {
            let store = self.store.clone();
            let turn_clone = turn.clone();
            let thread_clone = thread.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                store.save_turn(&turn_clone)?;
                store.save_thread(&thread_clone)?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow!("save turn completion panicked: {e}"))??;
        }

        self.emit_event(
            &thread_id,
            Some(&turn_id),
            None,
            "turn.completed",
            {
                let mut payload = json!({ "turn": turn.clone() });
                if let Some(ref summary) = turn_summary {
                    if let Some(obj) = payload.as_object_mut() {
                        obj.insert("turn_summary".to_string(), summary.clone());
                    }
                }
                payload
            },
        )
        .await?;

        {
            let mut active = self.active.lock().await;
            if let Some(state) = active.engines.get_mut(&thread_id)
                && state
                    .active_turn
                    .as_ref()
                    .is_some_and(|t| t.turn_id == turn_id)
            {
                state.active_turn = None;
            }
            touch_lru(&mut active.lru, &thread_id);
        }

        Ok(())
    }

    async fn save_item_and_attach_blocking(
        &self,
        item: &TurnItemRecord,
        turn_id: &str,
    ) -> Result<()> {
        let store = self.store.clone();
        let item = item.clone();
        let turn_id = turn_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            store.save_item(&item)?;
            let mut turn = store.load_turn(&turn_id)?;
            if !turn.item_ids.iter().any(|id| id == &item.id) {
                turn.item_ids.push(item.id.clone());
                store.save_turn(&turn)?;
            }
            Ok(())
        })
        .await
        .map_err(|e| anyhow!("save_item_and_attach panicked: {e}"))?
    }

    async fn update_and_save_item_blocking(
        &self,
        item_id: &str,
        update_fn: impl FnOnce(&mut TurnItemRecord),
    ) -> Result<TurnItemRecord> {
        let store = self.store.clone();
        let item_id = item_id.to_string();
        let mut item = tokio::task::spawn_blocking(move || store.load_item(&item_id))
            .await
            .map_err(|e| anyhow!("load_item panicked: {e}"))??;
        update_fn(&mut item);
        let store = self.store.clone();
        let item_clone = item.clone();
        tokio::task::spawn_blocking(move || store.save_item(&item_clone))
            .await
            .map_err(|e| anyhow!("save_item panicked: {e}"))??;
        Ok(item)
    }

    async fn is_interrupt_requested(&self, thread_id: &str, turn_id: &str) -> Result<bool> {
        let active = self.active.lock().await;
        let Some(state) = active.engines.get(thread_id) else {
            return Ok(false);
        };
        let Some(turn) = state.active_turn.as_ref() else {
            return Ok(false);
        };
        Ok(turn.turn_id == turn_id && turn.interrupt_requested)
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

pub(crate) fn touch_lru(lru: &mut VecDeque<String>, thread_id: &str) {
    if let Some(idx) = lru.iter().position(|id| id == thread_id) {
        lru.remove(idx);
    }
    lru.push_back(thread_id.to_string());
}

pub(crate) fn enforce_lru_capacity(
    active: &mut ActiveThreads,
    max_active_threads: usize,
) -> Vec<EngineHandle> {
    let mut evicted = Vec::new();
    if max_active_threads == 0 || active.engines.len() < max_active_threads {
        return evicted;
    }
    let protected = active
        .engines
        .iter()
        .filter_map(|(thread_id, state)| {
            if state.active_turn.is_some() {
                Some(thread_id.clone())
            } else {
                None
            }
        })
        .collect::<HashSet<_>>();

    let scan_limit = active.lru.len();
    for _ in 0..scan_limit {
        let Some(candidate) = active.lru.pop_front() else {
            break;
        };
        if protected.contains(&candidate) {
            active.lru.push_back(candidate);
            continue;
        }
        if let Some(state) = active.engines.remove(&candidate) {
            evicted.push(state.engine);
        }
        break;
    }
    evicted
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

