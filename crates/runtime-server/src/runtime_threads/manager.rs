//! Sidecar wrapper around orchestrator `RuntimeThreadManager` core (D16 E1-b phase 2).

use std::collections::{HashMap, VecDeque};
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
    RuntimeApprovalDecision, RuntimeEnginePolicy, RuntimeThreadManagerConfig, RuntimeThreadStore,
    RuntimeUserInputResponse,
};
use zagens_runtime_orchestrator::runtime_threads::manager::{
    RuntimeThreadManager as RuntimeThreadManagerCore, checklist_tool_needs_panel_push,
    scratchpad_tool_needs_panel_push,
};

pub type SharedRuntimeThreadManager = Arc<RuntimeThreadManager>;

type InnerManager = RuntimeThreadManagerCore<RuntimeEnginePolicy, RuntimeUserInputResponse>;

#[derive(Clone)]
struct ScratchpadStatusCacheEntry {
    fetched_at: Instant,
    status: Option<serde_json::Value>,
}

const SCRATCHPAD_STATUS_CACHE_TTL: Duration = Duration::from_secs(2);

/// Max harness node decisions retained per thread for the LHT "nodes" tab.
/// Bounded ring — the panel only shows a recent decision trail, the full log
/// lives in the persisted thread items / `sidecar.log`.
const MAX_HARNESS_NODE_RECORDS: usize = 80;

/// One harness node decision (`long_horizon.<kind>: {…}`) captured for the
/// LHT panel "nodes" tab, so the decision stream that previously only reached
/// `sidecar.log` is visible live in the UI (DEMO5 #3).
#[derive(Clone)]
pub(crate) struct HarnessNodeRecord {
    pub kind: String,
    pub payload: Value,
    pub ts_ms: i64,
}

/// Live LHT nudge telemetry, fed by `long_horizon.*` status events so the
/// task-graph panel reflects converted/blocked counts mid-turn without the
/// engine op loop (which is starved during a long turn).
#[derive(Clone, Default)]
pub(crate) struct HarnessTelemetryCacheEntry {
    pub emitted: u32,
    pub converted: u32,
    pub blocked: u32,
    pub nudge_count: u32,
    pub lht_blocked: bool,
    /// Bounded ring of recent harness node decisions (newest last).
    pub recent_nodes: VecDeque<HarnessNodeRecord>,
    /// Composable harness completion gate (P2 panel).
    pub completion_gate: crate::long_horizon::completion_gate_panel::CompletionGatePanelCache,
    /// Phase 4 macro loop (implement / craft / remediation).
    pub macro_loop: crate::long_horizon::macro_loop_panel::MacroLoopPanelCache,
}

/// Sidecar runtime thread manager — orchestrator core plus host-only services.
#[derive(Clone)]
pub struct RuntimeThreadManager {
    inner: InnerManager,
    pub(crate) config: Config,
    pub(crate) background: RuntimeThreadBackgroundSlots,
    checklist_cache: Arc<StdMutex<HashMap<String, String>>>,
    plan_cache: Arc<StdMutex<HashMap<String, String>>>,
    scratchpad_status_cache: Arc<StdMutex<HashMap<String, ScratchpadStatusCacheEntry>>>,
    harness_telemetry_cache: Arc<StdMutex<HashMap<String, HarnessTelemetryCacheEntry>>>,
    pub(crate) session_manager: Option<Arc<crate::SessionManager>>,
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
        Self::open_with_session_manager(config, workspace, manager_cfg, None)
    }

    pub fn open_with_session_manager(
        config: Config,
        workspace: PathBuf,
        manager_cfg: RuntimeThreadManagerConfig,
        session_manager: Option<Arc<crate::SessionManager>>,
    ) -> Result<Self> {
        let inner = InnerManager::open(workspace.clone(), manager_cfg)?;
        let manager = Self {
            inner,
            config,
            background: RuntimeThreadBackgroundSlots::new(),
            checklist_cache: Arc::new(StdMutex::new(HashMap::new())),
            plan_cache: Arc::new(StdMutex::new(HashMap::new())),
            scratchpad_status_cache: Arc::new(StdMutex::new(HashMap::new())),
            harness_telemetry_cache: Arc::new(StdMutex::new(HashMap::new())),
            session_manager,
        };
        let active_ids: Vec<String> = manager
            .store
            .list_threads()
            .unwrap_or_default()
            .into_iter()
            .flat_map(|t| t.scratchpad_history())
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
            plan_cache: Arc::new(StdMutex::new(HashMap::new())),
            scratchpad_status_cache: Arc::new(StdMutex::new(HashMap::new())),
            harness_telemetry_cache: Arc::new(StdMutex::new(HashMap::new())),
            session_manager: None,
        })
    }

    /// Read-only audit scratchpad progress for a thread (B5).
    pub fn get_thread_scratchpad_status(
        &self,
        thread_id: &str,
    ) -> Result<Option<serde_json::Value>> {
        if let Ok(cache) = self.scratchpad_status_cache.lock()
            && let Some(entry) = cache.get(thread_id)
            && entry.fetched_at.elapsed() < SCRATCHPAD_STATUS_CACHE_TTL
        {
            return Ok(entry.status.clone());
        }
        let thread = self.load_thread_sync(thread_id)?;
        if thread.scratchpad_history().is_empty() {
            return Ok(None);
        }
        let checklist_json = self.get_thread_checklist(thread_id);
        let out = crate::scratchpad::ui_status::build_thread_scratchpad_panel_status(
            &thread,
            checklist_json.as_deref(),
        );
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

    /// Bootstrap audit scratchpad for a thread (REST / Zagens Audit panel).
    pub fn init_thread_scratchpad(
        &self,
        thread_id: &str,
        run_id: Option<&str>,
        scope: Option<&str>,
        areas_json: Option<&[Value]>,
    ) -> Result<Value> {
        let mut thread = self.load_thread_sync(thread_id)?;
        let mut ctx = crate::tools::spec::ToolContext::new(&thread.workspace);
        ctx.runtime.wire.active_thread_id = Some(thread.id.clone());
        ctx.runtime.wire.active_task_id = thread.task_id.clone();

        let resolved_run = if let Some(rid) = run_id.map(str::trim).filter(|s| !s.is_empty()) {
            crate::scratchpad::validate_run_id(rid).map_err(|e| anyhow::anyhow!(e.to_string()))?;
            rid.to_string()
        } else if let Some(active) = thread.scratchpad_run_id.clone() {
            active
        } else {
            thread.id.clone()
        };

        let areas = match areas_json {
            Some(raw) if !raw.is_empty() => crate::scratchpad::parse_init_areas(raw)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?,
            _ => crate::scratchpad::default_init_areas(),
        };

        let _store = crate::scratchpad::ScratchpadStore::init(&ctx, &resolved_run, areas, scope)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        thread.record_scratchpad_run(&resolved_run);
        thread.updated_at = Utc::now();
        self.store.save_thread(&thread)?;

        if let Ok(mut cache) = self.scratchpad_status_cache.lock() {
            cache.remove(thread_id);
        }

        let checklist_json = self.get_thread_checklist(thread_id);
        crate::scratchpad::ui_status::build_thread_scratchpad_panel_status(
            &thread,
            checklist_json.as_deref(),
        )
        .ok_or_else(|| anyhow::anyhow!("scratchpad status unavailable after init"))
    }

    /// Return the cached checklist snapshot for a thread (for Zagens WebView panel).
    pub fn get_thread_checklist(&self, thread_id: &str) -> Option<String> {
        if let Ok(cache) = self.checklist_cache.lock()
            && let Some(json) = cache.get(thread_id)
        {
            return Some(json.clone());
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

    /// Return the cached plan snapshot for a thread (for Zagens harness task-graph).
    pub fn get_thread_plan(&self, thread_id: &str) -> Option<String> {
        if let Ok(cache) = self.plan_cache.lock()
            && let Some(json) = cache.get(thread_id)
        {
            return Some(json.clone());
        }
        let thread = self.store.load_thread(thread_id).ok()?;
        let json = thread
            .plan_snapshot
            .and_then(|v| serde_json::to_string(&v).ok())?;
        if let Ok(mut cache) = self.plan_cache.lock() {
            cache.insert(thread_id.to_string(), json.clone());
        }
        Some(json)
    }

    pub(crate) fn persist_thread_plan(&self, thread_id: &str, plan_json: &str) {
        if let Ok(mut cache) = self.plan_cache.lock() {
            cache.insert(thread_id.to_string(), plan_json.to_string());
        }
        let snapshot: Option<serde_json::Value> = serde_json::from_str(plan_json).ok();
        let store = self.store.clone();
        let thread_id_owned = thread_id.to_string();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let thread_id = thread_id_owned;
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(mut thread) = store.load_thread(&thread_id) {
                        thread.plan_snapshot = snapshot;
                        thread.updated_at = Utc::now();
                        if store.save_thread(&thread).is_err() {
                            tracing::warn!(%thread_id, "failed to persist plan snapshot on thread");
                        }
                    }
                })
                .await;
            });
        } else if let Ok(mut thread) = self.store.load_thread(thread_id) {
            thread.plan_snapshot = snapshot;
            thread.updated_at = Utc::now();
            if self.store.save_thread(&thread).is_err() {
                tracing::warn!(thread_id, "failed to persist plan snapshot on thread");
            }
        }
    }

    /// Derived LHT task graph — built from persisted plan/checklist snapshots
    /// plus cached nudge telemetry. Intentionally avoids the engine op loop:
    /// during a long turn that loop is busy and a live query would time out,
    /// blanking the panel exactly when observability matters most (§4.9).
    pub async fn get_thread_harness_task_graph(&self, thread_id: &str) -> Result<Value> {
        let plan_json = self.get_thread_plan(thread_id);
        let plan = plan_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .map(|v| crate::long_horizon::snapshots::plan_from_json(Some(&v)))
            .unwrap_or_else(crate::long_horizon::snapshots::empty_plan_snapshot);
        let checklist_json = self.get_thread_checklist(thread_id);
        let checklist_value = checklist_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok());
        let checklist =
            crate::long_horizon::snapshots::checklist_from_json(checklist_value.as_ref());
        let lht = self.config.long_horizon_config();

        let cached = self
            .harness_telemetry_cache
            .lock()
            .ok()
            .and_then(|c| c.get(thread_id).cloned());
        let (lht_blocked, nudge_count, telemetry) = cached
            .as_ref()
            .map(|e| {
                let conversion_pct = if e.emitted == 0 {
                    0
                } else {
                    e.converted
                        .saturating_mul(100)
                        .saturating_div(e.emitted)
                        .min(100) as u8
                };
                (
                    Some(e.lht_blocked),
                    Some(e.nudge_count),
                    Some(crate::long_horizon::TaskGraphTelemetryJson {
                        emitted: e.emitted,
                        converted: e.converted,
                        blocked: e.blocked,
                        conversion_pct,
                    }),
                )
            })
            .unwrap_or((None, None, None));

        let mut completion_gate = cached.as_ref().map(|e| {
            crate::long_horizon::completion_gate_panel::merge_completion_gate_panel(
                &e.completion_gate,
                None,
            )
        });
        if lht.completion_gate.is_active() {
            let mode = match lht.completion_gate.mode {
                zagens_core::long_horizon::CompletionGateMode::Enforce => "enforce",
                zagens_core::long_horizon::CompletionGateMode::Observe => "observe",
            };
            let generic_mode = |m: zagens_core::long_horizon::GenericGateMode| match m {
                zagens_core::long_horizon::GenericGateMode::Enforce => Some("enforce".to_string()),
                zagens_core::long_horizon::GenericGateMode::Observe => Some("observe".to_string()),
                zagens_core::long_horizon::GenericGateMode::Off => None,
            };
            let replay = generic_mode(lht.completion_gate.auto_verify_replay);
            let toolchain = generic_mode(lht.completion_gate.toolchain_gate);
            match &mut completion_gate {
                Some(cg) => {
                    cg.active = true;
                    if cg.mode.is_none() {
                        cg.mode = Some(mode.to_string());
                    }
                    cg.auto_verify_replay = replay;
                    cg.toolchain_gate = toolchain;
                }
                None => {
                    completion_gate = Some(crate::long_horizon::CompletionGatePanelJson {
                        active: true,
                        mode: Some(mode.to_string()),
                        auto_verify_replay: replay,
                        toolchain_gate: toolchain,
                        ..Default::default()
                    });
                }
            }
        }
        let macro_loop = cached.as_ref().map(|e| {
            crate::long_horizon::macro_loop_panel::merge_macro_loop_panel(
                lht.macro_loop.enabled,
                &e.macro_loop,
                None,
            )
        });
        let mut value = crate::long_horizon::build_task_graph_value_with_telemetry(
            &plan,
            &checklist,
            "en",
            &lht,
            lht_blocked,
            nudge_count,
            telemetry,
            completion_gate,
            macro_loop,
        );
        // Attach the recent harness node-decision trail (newest last) for the
        // LHT panel "nodes" tab (DEMO5 #3). Pure read of the live cache; empty
        // when no node has fired yet.
        if let (Some(obj), Some(entry)) = (value.as_object_mut(), cached.as_ref()) {
            let nodes: Vec<Value> = entry
                .recent_nodes
                .iter()
                .map(|n| {
                    json!({
                        "kind": n.kind,
                        "ts_ms": n.ts_ms,
                        "payload": n.payload,
                    })
                })
                .collect();
            obj.insert("recent_nodes".to_string(), Value::Array(nodes));
        }
        Ok(value)
    }

    /// Fold a `long_horizon.*` status line into the live telemetry cache.
    /// Returns `true` when the cache changed (caller should push a panel frame).
    pub(crate) fn update_harness_telemetry_from_status(
        &self,
        thread_id: &str,
        message: &str,
    ) -> bool {
        let payload = message
            .find('{')
            .and_then(|i| serde_json::from_str::<Value>(&message[i..]).ok());
        let Ok(mut cache) = self.harness_telemetry_cache.lock() else {
            return false;
        };
        let entry = cache.entry(thread_id.to_string()).or_default();
        // Record the node decision for the "nodes" tab regardless of kind: the
        // kind is the token between `long_horizon.` and the first `:`/space.
        let mut node_recorded = false;
        if let Some(rest) = message.strip_prefix("long_horizon.") {
            let kind = rest
                .split(|c: char| c == ':' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_string();
            if !kind.is_empty() {
                entry.recent_nodes.push_back(HarnessNodeRecord {
                    kind,
                    payload: payload.clone().unwrap_or(Value::Null),
                    ts_ms: Utc::now().timestamp_millis(),
                });
                while entry.recent_nodes.len() > MAX_HARNESS_NODE_RECORDS {
                    entry.recent_nodes.pop_front();
                }
                node_recorded = true;
            }
        }
        entry
            .completion_gate
            .apply_status(message, payload.as_ref());
        entry.macro_loop.apply_status(message, payload.as_ref());
        if message.starts_with("long_horizon.continue_injected") {
            if let Some(p) = payload.as_ref() {
                if let Some(v) = p.get("emitted").and_then(Value::as_u64) {
                    entry.emitted = v as u32;
                }
                if let Some(v) = p.get("converted").and_then(Value::as_u64) {
                    entry.converted = v as u32;
                }
                if let Some(v) = p.get("nudge_count").and_then(Value::as_u64) {
                    entry.nudge_count = v as u32;
                }
            }
            entry.lht_blocked = false;
            true
        } else if message.starts_with("long_horizon.nudge_outcome") {
            if let Some(v) = payload
                .as_ref()
                .and_then(|p| p.get("converted"))
                .and_then(Value::as_u64)
            {
                entry.converted = v as u32;
            }
            true
        } else if message.starts_with("long_horizon.blocked") {
            entry.blocked = entry.blocked.saturating_add(1);
            entry.lht_blocked = true;
            true
        } else {
            node_recorded
        }
    }

    /// Cycle briefings — live engine or on-disk archives (thread id = session id).
    pub async fn get_thread_harness_cycles(&self, thread_id: &str) -> Result<Value> {
        {
            let active = self.active.lock().await;
            if let Some(state) = active.engines.get(thread_id) {
                let engine = state.engine.clone();
                drop(active);
                return engine.query_harness_cycles().await;
            }
        }
        let archives = crate::cycle_manager::list_cycle_archive_summaries(thread_id);
        let model = self.store.load_thread(thread_id).ok().map(|t| t.model);
        let configured_threshold = model
            .as_deref()
            .and_then(|m| u32::try_from(self.config.cycle_runtime_config(m).threshold_for(m)).ok());
        Ok(crate::long_horizon::build_cycles_value(
            0,
            &[],
            &archives,
            None,
            model.as_deref(),
            configured_threshold,
        ))
    }

    /// Zagens panel channel (C): push harness task-graph on SSE.
    pub(crate) async fn emit_panel_harness_task_graph(
        &self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<()> {
        let graph = self.get_thread_harness_task_graph(thread_id).await?;
        self.emit_event(
            thread_id,
            Some(turn_id),
            None,
            "harness.task_graph",
            json!({ "task_graph": graph }),
        )
        .await?;
        Ok(())
    }

    /// Zagens panel channel (C): push plan snapshot on SSE (symmetric with checklist).
    pub(crate) async fn emit_panel_plan(&self, thread_id: &str, turn_id: &str) -> Result<()> {
        let Some(json_str) = self.get_thread_plan(thread_id) else {
            return Ok(());
        };
        let plan =
            serde_json::from_str::<Value>(&json_str).unwrap_or_else(|_| json!({ "raw": json_str }));
        self.emit_event(
            thread_id,
            Some(turn_id),
            None,
            "panel.plan",
            json!({ "plan": plan }),
        )
        .await?;
        Ok(())
    }

    /// Zagens panel channel (C): push checklist snapshot on the live SSE stream (B-channel fallback).
    pub(crate) async fn emit_panel_checklist(&self, thread_id: &str, turn_id: &str) -> Result<()> {
        let Some(json_str) = self.get_thread_checklist(thread_id) else {
            return Ok(());
        };
        let checklist =
            serde_json::from_str::<Value>(&json_str).unwrap_or_else(|_| json!({ "raw": json_str }));
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

    /// Zagens panel channel (C): push a pre-computed context snapshot on SSE.
    ///
    /// Unlike [`Self::emit_panel_context`], this does NOT re-query the engine —
    /// the engine has already serialized its live `ThreadContextSnapshot` and
    /// handed it over via the harness-status channel. Used to keep the Context
    /// tab live mid-turn, when the op-loop `QueryContext` is starved and the
    /// query-based path would fall back to a stale store snapshot.
    pub(crate) async fn emit_panel_context_snapshot(
        &self,
        thread_id: &str,
        turn_id: &str,
        snapshot_json: &str,
    ) -> Result<()> {
        let snapshot: serde_json::Value = serde_json::from_str(snapshot_json)?;
        self.emit_event(
            thread_id,
            Some(turn_id),
            None,
            "panel.context",
            json!({ "context": snapshot }),
        )
        .await?;
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
        use super::{ActiveThreadState, touch_lru};
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
