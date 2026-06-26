//! Generic runtime thread manager core (D16 E1-b phase 2).
//!
//! Sidecar-specific `Config`, task/automation managers, and scratchpad UI hooks
//! live in `zagens-cli` as a wrapper around this type.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use serde_json::Value;
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;

use crate::engine::Op;
use crate::models::Message;

use super::active::{ActiveThreads, RuntimeApprovalDecision};
use super::persist::{RuntimeThreadStore, reconstruct_messages_for_store};
use super::routing::load_routing_rules;
use super::thread_status::ThreadStatusOwner;
use super::turn_coordinator::TurnCoordinator;
use super::types::*;
use super::{RoutingRule, RuntimeThreadManagerConfig};

pub const EVENT_CHANNEL_CAPACITY: usize = 4096;
pub const RUNTIME_RESTART_REASON: &str = "Interrupted by process restart";

/// Manages durable thread/turn state, active engine handles, and event broadcast.
///
/// # Lock ordering invariant
///
/// Two `Mutex`es exist across this module:
/// - `RuntimeThreadStore::state` — protects the monotonic event sequence counter.
/// - `RuntimeThreadManager::active` — protects the set of loaded engine handles.
///
/// **No code path holds both locks simultaneously.**
#[derive(Clone)]
pub struct RuntimeThreadManager<P, R> {
    pub workspace: PathBuf,
    pub store: RuntimeThreadStore,
    pub active: Arc<Mutex<ActiveThreads<P, R>>>,
    event_tx: broadcast::Sender<RuntimeEventRecord>,
    pub manager_cfg: RuntimeThreadManagerConfig,
    pub cancel_token: CancellationToken,
    pub routing_rules: Arc<Mutex<Vec<RoutingRule>>>,
    pub routing_rules_path: PathBuf,
    pub coordinators: Arc<Mutex<TurnCoordinator>>,
    pub thread_status: ThreadStatusOwner,
}

impl<P, R> RuntimeThreadManager<P, R>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
{
    pub fn open(workspace: PathBuf, manager_cfg: RuntimeThreadManagerConfig) -> Result<Self> {
        let store = RuntimeThreadStore::open(manager_cfg.data_dir.clone())?;
        Self::open_with_store(workspace, manager_cfg, store)
    }

    pub fn open_with_store(
        workspace: PathBuf,
        manager_cfg: RuntimeThreadManagerConfig,
        store: RuntimeThreadStore,
    ) -> Result<Self> {
        let (event_tx, _event_rx) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let routing_rules_path = manager_cfg.data_dir.join("routing_rules.json");
        let routing_rules = load_routing_rules(&routing_rules_path).unwrap_or_default();
        let manager = Self {
            workspace,
            store,
            active: Arc::new(Mutex::new(ActiveThreads::default())),
            event_tx,
            manager_cfg,
            cancel_token: CancellationToken::new(),
            routing_rules: Arc::new(Mutex::new(routing_rules)),
            routing_rules_path,
            coordinators: Arc::new(Mutex::new(TurnCoordinator::default())),
            thread_status: ThreadStatusOwner::new(),
        };
        manager.recover_interrupted_state()?;
        Ok(manager)
    }

    pub fn shutdown(&self) {
        self.cancel_token.cancel();
    }

    pub fn is_shutdown(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    #[must_use]
    pub fn subscribe_events(&self) -> broadcast::Receiver<RuntimeEventRecord> {
        self.event_tx.subscribe()
    }

    pub async fn emit_event(
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

    pub fn events_since(
        &self,
        thread_id: &str,
        since_seq: Option<u64>,
    ) -> Result<Vec<RuntimeEventRecord>> {
        self.store.events_since(thread_id, since_seq)
    }

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

    pub fn resolve_thread_workspace_path(manager_workspace: &Path, raw: &str) -> Result<PathBuf> {
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

    pub async fn unload_idle_thread_engine(&self, thread_id: &str) -> Result<()> {
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
        let messages = reconstruct_messages_for_store(&self.store, &turns)?;
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
        remember_for_session: bool,
    ) -> Result<()> {
        let (engine, approval_key) = {
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
            let approval_key = pending.approval_key.clone();
            let state = active
                .engines
                .get(thread_id)
                .ok_or_else(|| anyhow!("engine not found for {thread_id}"))?;
            (state.engine.clone(), approval_key)
        };

        if approved {
            engine
                .approve_tool_call_with_options(
                    tool_call_id,
                    Some(approval_key),
                    remember_for_session,
                )
                .await?;
            self.emit_thread_status(
                thread_id,
                Some(turn_id),
                super::thread_status::ThreadStreamStatus::Streaming,
            )
            .await?;
        } else {
            engine.deny_tool_call(tool_call_id).await?;
        }
        Ok(())
    }

    pub fn approval_decision(
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
        // Runs synchronously during `open()` before the Tokio runtime is available;
        // SQLite recovery must complete before serving thread APIs.
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
}

#[must_use]
pub fn tool_kind_for_name(name: &str) -> TurnItemKind {
    let lower = name.to_ascii_lowercase();
    if lower == "exec_shell" || lower == "exec_shell_wait" || lower == "exec_shell_interact" {
        return TurnItemKind::CommandExecution;
    }
    if lower.contains("patch") || lower.contains("write") || lower.contains("edit") {
        return TurnItemKind::FileChange;
    }
    TurnItemKind::ToolCall
}

pub fn scratchpad_tool_needs_panel_push(name: &str) -> bool {
    name.starts_with("scratchpad_")
}

pub fn checklist_tool_needs_panel_push(name: &str) -> bool {
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
