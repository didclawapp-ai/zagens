//! Sidecar wrapper around orchestrator `RuntimeThreadManager` core (D16 E1-b phase 2).

use std::collections::HashMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Utc;
use serde_json::{Value, json};

use crate::agent_surface::AppMode;
use crate::config::Config;

use super::background_slots::RuntimeThreadBackgroundSlots;
use super::{
    ActiveThreadState, RuntimeApprovalDecision, RuntimeEnginePolicy, RuntimeThreadStore,
    RuntimeUserInputResponse, RuntimeThreadManagerConfig,
};
use deepseek_runtime_orchestrator::runtime_threads::manager::{
    RuntimeThreadManager as RuntimeThreadManagerCore, checklist_tool_needs_panel_push,
    scratchpad_tool_needs_panel_push,
};

pub use deepseek_runtime_orchestrator::runtime_threads::manager::tool_kind_for_name;

pub type SharedRuntimeThreadManager = Arc<RuntimeThreadManager>;

type InnerManager = RuntimeThreadManagerCore<RuntimeEnginePolicy, RuntimeUserInputResponse>;

#[derive(Clone)]
struct ScratchpadStatusCacheEntry {
    fetched_at: Instant,
    status: Option<serde_json::Value>,
}

const SCRATCHPAD_STATUS_CACHE_TTL: Duration = Duration::from_secs(2);

/// Sidecar runtime thread manager — orchestrator core plus host-only services.
#[derive(Clone)]
pub struct RuntimeThreadManager {
    inner: InnerManager,
    pub(crate) config: Config,
    pub(crate) background: RuntimeThreadBackgroundSlots,
    checklist_cache: Arc<StdMutex<HashMap<String, String>>>,
    scratchpad_status_cache: Arc<StdMutex<HashMap<String, ScratchpadStatusCacheEntry>>>,
}

impl Deref for RuntimeThreadManager {
    type Target = InnerManager;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl RuntimeThreadManager {
    pub fn resolve_thread_workspace_path(manager_workspace: &Path, raw: &str) -> Result<PathBuf> {
        InnerManager::resolve_thread_workspace_path(manager_workspace, raw)
    }

    pub fn approval_decision(
        auto_approve: bool,
        trust_mode: bool,
        requires_full_access: bool,
    ) -> RuntimeApprovalDecision {
        InnerManager::approval_decision(auto_approve, trust_mode, requires_full_access)
    }

    pub fn open(
        config: Config,
        workspace: PathBuf,
        manager_cfg: RuntimeThreadManagerConfig,
    ) -> Result<Self> {
        let inner = InnerManager::open(workspace.clone(), manager_cfg)?;
        let manager = Self {
            inner,
            config,
            background: RuntimeThreadBackgroundSlots::new(),
            checklist_cache: Arc::new(StdMutex::new(HashMap::new())),
            scratchpad_status_cache: Arc::new(StdMutex::new(HashMap::new())),
        };
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

    pub(crate) fn open_with_store(
        config: Config,
        workspace: PathBuf,
        manager_cfg: RuntimeThreadManagerConfig,
        store: RuntimeThreadStore,
    ) -> Result<Self> {
        let inner = InnerManager::open_with_store(workspace, manager_cfg, store)?;
        Ok(Self {
            inner,
            config,
            background: RuntimeThreadBackgroundSlots::new(),
            checklist_cache: Arc::new(StdMutex::new(HashMap::new())),
            scratchpad_status_cache: Arc::new(StdMutex::new(HashMap::new())),
        })
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
        scratchpad_tool_needs_panel_push(name)
    }

    pub(crate) fn checklist_tool_needs_panel_push(name: &str) -> bool {
        checklist_tool_needs_panel_push(name)
    }

    /// Attach the durable task manager so model-visible task tools work inside
    /// runtime thread turns as well as interactive TUI turns.
    pub fn attach_task_manager(&self, task_manager: crate::task_manager::SharedTaskManager) {
        self.background.attach_task_manager(task_manager);
    }

    /// Attach the automation manager for model-visible scheduling tools.
    pub fn attach_automation_manager(
        &self,
        automations: crate::automation_manager::SharedAutomationManager,
    ) {
        self.background.attach_automation_manager(automations);
    }

    #[cfg(test)]
    pub(crate) async fn install_test_engine(
        &self,
        thread_id: &str,
        engine: crate::core::engine::EngineHandle,
    ) -> Result<()> {
        use super::{touch_lru, ActiveThreadState};
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
