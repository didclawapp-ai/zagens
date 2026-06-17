//! In-process thread/engine lifecycle for the TUI (RuntimeThreadManager).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use tokio::sync::broadcast;

use crate::cli::args::Cli;
use crate::cli::context::CliContext;
use crate::core::engine::EngineHandle;
use crate::core::events::Event;
use crate::runtime_threads::event_coalesce::coalesce_delta_events;
use crate::runtime_threads::{
    CreateThreadRequest, RuntimeEventRecord, RuntimeThreadManager, RuntimeThreadManagerConfig,
    SharedRuntimeThreadManager, StartTurnRequest, ThreadListFilter, ThreadRecord,
    UpdateThreadRequest,
};
use crate::task_manager::TaskManagerConfig;
use crate::task_type::TaskType;

use super::approval_policy::{self, policy_cyclable, policy_display_label};
use super::harness::{ChecklistSnapshot, parse_checklist_json, parse_checklist_panel_payload};
use super::left_rail::SessionList;
use super::task_graph::{
    TaskGraphSnapshot, parse_task_graph_panel_payload, parse_task_graph_value,
};
use super::transcript::TranscriptItem;
use super::transcript_history::{default_history_turn_limit, seed_from_thread_store};

/// Runtime thread events plus immediate panel payloads for the TUI.
pub struct RuntimeUiDelta {
    pub events: Vec<Event>,
    /// Latest checklist from a `panel.checklist` event in this batch.
    pub checklist: Option<ChecklistSnapshot>,
    /// Latest task graph from `harness.task_graph` in this batch.
    pub task_graph: Option<TaskGraphSnapshot>,
}

impl RuntimeUiDelta {
    fn empty() -> Self {
        Self {
            events: Vec::new(),
            checklist: None,
            task_graph: None,
        }
    }
}

fn checklist_from_record(record: &RuntimeEventRecord) -> Option<ChecklistSnapshot> {
    if record.event != "panel.checklist" {
        return None;
    }
    parse_checklist_panel_payload(&record.payload).or_else(|| {
        record
            .payload
            .get("checklist")
            .and_then(|v| serde_json::to_string(v).ok())
            .and_then(|json| parse_checklist_json(&json))
    })
}

fn task_graph_from_record(record: &RuntimeEventRecord) -> Option<TaskGraphSnapshot> {
    if record.event != "harness.task_graph" {
        return None;
    }
    parse_task_graph_panel_payload(&record.payload)
}

fn ingest_record(
    record: &RuntimeEventRecord,
    thread_id: &str,
    last_event_seq: &mut u64,
    events: &mut Vec<Event>,
    checklist: &mut Option<ChecklistSnapshot>,
    task_graph: &mut Option<TaskGraphSnapshot>,
) {
    if record.thread_id != thread_id || record.seq <= *last_event_seq {
        return;
    }
    *last_event_seq = record.seq;
    if let Some(snap) = checklist_from_record(record) {
        *checklist = Some(snap);
    }
    if let Some(graph) = task_graph_from_record(record) {
        *task_graph = Some(graph);
    }
    if let Some(ev) = super::runtime_events::map_record(record) {
        events.push(ev);
    }
}

fn effective_auto_approve(yolo: bool, thread: &ThreadRecord, policy: &str) -> bool {
    yolo || thread.auto_approve || approval_policy::auto_approve_for_turn(policy, false)
}

pub struct TuiSessionHost {
    pub manager: SharedRuntimeThreadManager,
    pub thread: ThreadRecord,
    pub yolo: bool,
    /// Effective `auto_approve` for the next turn (policy, YOLO, or session remember).
    pub auto_approve: bool,
    /// Global approval policy (`config.toml` → `approval_policy`), same as desktop Settings.
    pub approval_policy: String,
    workspace_filter: std::path::PathBuf,
    workspace_filter_canon: std::path::PathBuf,
    last_event_seq: u64,
    event_rx: broadcast::Receiver<RuntimeEventRecord>,
}

impl TuiSessionHost {
    pub async fn open(ctx: &mut CliContext, cli: &Cli) -> Result<Self> {
        inject_desktop_api_key(&mut ctx.config);
        let task_cfg = TaskManagerConfig::from_runtime(
            &ctx.config,
            ctx.workspace.clone(),
            ctx.config.default_text_model.clone(),
            None,
        );
        let manager_cfg = RuntimeThreadManagerConfig::from_task_data_dir(task_cfg.data_dir.clone());
        let manager = Arc::new(RuntimeThreadManager::open(
            ctx.config.clone(),
            ctx.workspace.clone(),
            manager_cfg,
        )?);

        let layout_prefs = super::layout::TuiLayoutPrefs::load();
        let thread =
            resolve_thread(&manager, ctx, cli, layout_prefs.last_thread_id.as_deref()).await?;
        let thread = ensure_tui_code_task_type(&manager, thread).await?;
        manager.resume_thread(&thread.id).await?;

        let event_rx = manager.subscribe_events();
        let approval_policy = manager
            .config
            .approval_policy
            .as_deref()
            .map(approval_policy::normalize_policy)
            .unwrap_or("on-request")
            .to_string();
        let auto_approve = effective_auto_approve(cli.yolo, &thread, &approval_policy);
        let workspace_filter = ctx.workspace.clone();
        let workspace_filter_canon =
            std::fs::canonicalize(&workspace_filter).unwrap_or_else(|_| workspace_filter.clone());
        let mut host = Self {
            manager,
            thread,
            yolo: cli.yolo,
            auto_approve,
            approval_policy,
            workspace_filter,
            workspace_filter_canon,
            last_event_seq: 0,
            event_rx,
        };
        host.sync_event_cursor();
        Ok(host)
    }

    pub fn thread_id(&self) -> &str {
        &self.thread.id
    }

    pub fn workspace_display(&self) -> String {
        crate::cli::context::display_path(&self.thread.workspace)
    }

    pub fn config(&self) -> &crate::config::Config {
        &self.manager.config
    }

    pub async fn engine_handle(&self) -> Option<EngineHandle> {
        let active = self.manager.active.lock().await;
        active
            .engines
            .get(&self.thread.id)
            .map(|state| state.engine.clone())
    }

    pub async fn send_prompt(&self, prompt: &str) -> Result<()> {
        let prompt = prompt.trim();
        if prompt.is_empty() {
            bail!("prompt is empty");
        }
        let mode = if self.yolo { "yolo" } else { "agent" };
        let auto = effective_auto_approve(self.yolo, &self.thread, &self.approval_policy);
        let req = StartTurnRequest {
            prompt: prompt.to_string(),
            mode: Some(mode.to_string()),
            auto_approve: Some(auto),
            allow_shell: Some(self.yolo || auto || self.manager.config.allow_shell()),
            trust_mode: Some(self.yolo || auto),
            ..Default::default()
        };
        self.manager
            .start_turn(&self.thread.id, req)
            .await
            .context("start_turn failed")?;
        Ok(())
    }

    pub async fn active_turn_id(&self) -> Option<String> {
        let active = self.manager.active.lock().await;
        active
            .engines
            .get(&self.thread.id)
            .and_then(|state| state.active_turn.as_ref().map(|t| t.turn_id.clone()))
    }

    pub async fn steer_prompt(&self, prompt: &str) -> Result<()> {
        let prompt = prompt.trim();
        if prompt.is_empty() {
            bail!("prompt is empty");
        }
        let turn_id = self
            .active_turn_id()
            .await
            .context("no active turn to steer")?;
        self.manager
            .steer_turn(
                &self.thread.id,
                &turn_id,
                crate::runtime_threads::SteerTurnRequest {
                    prompt: prompt.to_string(),
                },
            )
            .await
            .context("steer_turn failed")?;
        Ok(())
    }

    pub async fn interrupt_turn(&self) -> Result<()> {
        if let Some(handle) = self.engine_handle().await {
            handle.cancel();
        }
        let turn_id = {
            let active = self.manager.active.lock().await;
            active
                .engines
                .get(&self.thread.id)
                .and_then(|state| state.active_turn.as_ref().map(|t| t.turn_id.clone()))
        };
        if let Some(turn_id) = turn_id {
            let _ = self.manager.interrupt_turn(&self.thread.id, &turn_id).await;
        }
        Ok(())
    }

    pub async fn approve_tool(&self, id: &str, remember_session: bool) -> Result<()> {
        let handle = self
            .engine_handle()
            .await
            .context("no engine for approval")?;
        if remember_session {
            handle
                .approve_tool_call_with_options(id, None, true)
                .await?;
        } else {
            handle.approve_tool_call(id).await?;
        }
        Ok(())
    }

    pub fn approval_footer_meta(&self) -> (String, bool) {
        if self.yolo {
            return ("Auto".to_string(), false);
        }
        let label = policy_display_label(&self.approval_policy).to_string();
        (label, policy_cyclable(self.yolo))
    }

    pub async fn cycle_approval_policy(&mut self) -> Result<String> {
        if !policy_cyclable(self.yolo) {
            return Ok(self.approval_policy.clone());
        }
        let next = approval_policy::next_policy(&self.approval_policy);
        approval_policy::persist_to_config(next)?;
        self.approval_policy = next.to_string();
        self.auto_approve = effective_auto_approve(self.yolo, &self.thread, &self.approval_policy);
        self.thread = self
            .manager
            .update_thread(
                &self.thread.id,
                UpdateThreadRequest {
                    auto_approve: Some(self.auto_approve),
                    ..Default::default()
                },
            )
            .await?;
        Ok(self.approval_policy.clone())
    }

    pub async fn switch_model(&mut self, model: String) -> Result<ThreadRecord> {
        let model = model.trim().to_string();
        if model.is_empty() {
            bail!("model id is empty");
        }
        self.thread = self
            .manager
            .update_thread(
                &self.thread.id,
                UpdateThreadRequest {
                    model: Some(model),
                    ..Default::default()
                },
            )
            .await?;
        Ok(self.thread.clone())
    }

    pub fn model_catalog(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut push = |m: &str| {
            let m = m.trim();
            if m.is_empty() {
                return;
            }
            if out.iter().any(|x: &String| x.eq_ignore_ascii_case(m)) {
                return;
            }
            out.push(m.to_string());
        };
        push("auto");
        push(&self.thread.model);
        if let Some(m) = self.config().default_text_model.as_deref() {
            push(m);
        }
        for m in crate::config::COMMON_DEEPSEEK_MODELS {
            push(m);
        }
        out
    }

    pub async fn deny_tool(&self, id: &str) -> Result<()> {
        let handle = self
            .engine_handle()
            .await
            .context("no engine for approval")?;
        handle.deny_tool_call(id).await?;
        Ok(())
    }

    pub fn load_history(&self) -> Result<Vec<TranscriptItem>> {
        let turns = self
            .manager
            .store
            .list_turns_for_thread(&self.thread.id)
            .context("list turns")?;
        let events = self
            .manager
            .events_since(&self.thread.id, None)
            .unwrap_or_default();
        Ok(seed_from_thread_store(
            &self.manager.store,
            &turns,
            &events,
            default_history_turn_limit(),
        ))
    }

    pub async fn list_workspace_threads(&self) -> Result<Vec<ThreadRecord>> {
        let threads = self
            .manager
            .list_threads(ThreadListFilter::ActiveOnly, None)
            .await?;
        let workspace_canon = &self.workspace_filter_canon;
        let mut out: Vec<_> = threads
            .into_iter()
            .filter(|t| {
                let tw =
                    std::fs::canonicalize(&t.workspace).unwrap_or_else(|_| t.workspace.clone());
                tw == *workspace_canon
            })
            .collect();
        out.sort_by_key(|t| std::cmp::Reverse(t.updated_at));
        Ok(out)
    }

    /// Left-rail session rows with display names (thread title or latest turn summary).
    pub async fn workspace_session_list(
        &self,
        active_id: &str,
        locale: crate::localization::Locale,
    ) -> SessionList {
        let threads = self.list_workspace_threads().await.unwrap_or_default();
        let mut turn_summaries = std::collections::HashMap::new();
        for thread in &threads {
            if thread
                .title
                .as_ref()
                .is_none_or(|title| title.trim().is_empty())
                && let Ok(turns) = self.manager.store.list_turns_for_thread(&thread.id)
                && let Some(summary) = turns
                    .last()
                    .map(|turn| turn.input_summary.as_str())
                    .filter(|summary| !summary.trim().is_empty())
            {
                turn_summaries.insert(thread.id.clone(), summary.to_string());
            }
        }
        SessionList::from_threads_with_summaries(threads, active_id, &turn_summaries, locale)
    }

    pub async fn switch_thread(&mut self, thread_id: &str) -> Result<()> {
        let thread = self.manager.get_thread(thread_id).await?;
        let thread = ensure_tui_code_task_type(&self.manager, thread).await?;
        self.manager.resume_thread(&thread.id).await?;
        self.thread = thread;
        self.auto_approve = effective_auto_approve(self.yolo, &self.thread, &self.approval_policy);
        self.resubscribe_events();
        Ok(())
    }

    pub async fn switch_workspace(&mut self, workspace: PathBuf) -> Result<()> {
        self.workspace_filter = workspace.clone();
        self.workspace_filter_canon =
            std::fs::canonicalize(&workspace).unwrap_or_else(|_| workspace.clone());
        let thread = match resolve_latest(&self.manager, &workspace).await {
            Ok(thread) => thread,
            Err(_) => {
                let mode = if self.yolo { "yolo" } else { "agent" };
                self.manager
                    .create_thread(tui_create_thread_request(
                        workspace.clone(),
                        mode,
                        self.yolo,
                        self.yolo || self.manager.config.allow_shell(),
                    ))
                    .await?
            }
        };
        let thread = ensure_tui_code_task_type(&self.manager, thread).await?;
        self.manager.resume_thread(&thread.id).await?;
        self.thread = thread;
        self.auto_approve = effective_auto_approve(self.yolo, &self.thread, &self.approval_policy);
        self.resubscribe_events();
        Ok(())
    }

    pub async fn new_session(&mut self, ctx: &CliContext) -> Result<()> {
        let mode = if self.yolo { "yolo" } else { "agent" };
        let thread = self
            .manager
            .create_thread(tui_create_thread_request(
                ctx.workspace.clone(),
                mode,
                self.yolo,
                self.yolo || ctx.config.allow_shell(),
            ))
            .await?;
        self.manager.resume_thread(&thread.id).await?;
        self.thread = thread;
        self.auto_approve = effective_auto_approve(self.yolo, &self.thread, &self.approval_policy);
        self.resubscribe_events();
        Ok(())
    }

    /// Receive runtime thread events (broadcast + store catch-up). Does not read `engine.rx_event`.
    pub async fn recv_runtime_ui_delta(&mut self) -> RuntimeUiDelta {
        let mut delta = RuntimeUiDelta::empty();
        match self.event_rx.recv().await {
            Ok(record) => {
                ingest_record(
                    &record,
                    &self.thread.id,
                    &mut self.last_event_seq,
                    &mut delta.events,
                    &mut delta.checklist,
                    &mut delta.task_graph,
                );
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {
                delta = self.catch_up_ui_delta().await;
            }
            Err(broadcast::error::RecvError::Closed) => {}
        }
        while let Ok(record) = self.event_rx.try_recv() {
            ingest_record(
                &record,
                &self.thread.id,
                &mut self.last_event_seq,
                &mut delta.events,
                &mut delta.checklist,
                &mut delta.task_graph,
            );
        }
        delta
    }

    async fn catch_up_ui_delta(&mut self) -> RuntimeUiDelta {
        let records = self
            .manager
            .events_since_async(&self.thread.id, Some(self.last_event_seq))
            .await
            .unwrap_or_default();
        let mut delta = RuntimeUiDelta::empty();
        for record in coalesce_delta_events(records) {
            ingest_record(
                &record,
                &self.thread.id,
                &mut self.last_event_seq,
                &mut delta.events,
                &mut delta.checklist,
                &mut delta.task_graph,
            );
        }
        delta
    }

    fn sync_event_cursor(&mut self) {
        self.last_event_seq = self
            .manager
            .events_since(&self.thread.id, None)
            .ok()
            .and_then(|events| events.last().map(|e| e.seq))
            .unwrap_or(0);
    }

    fn resubscribe_events(&mut self) {
        self.event_rx = self.manager.subscribe_events();
        self.sync_event_cursor();
    }

    pub fn fetch_checklist(&self) -> Option<ChecklistSnapshot> {
        self.manager
            .get_thread_checklist(&self.thread.id)
            .and_then(|json| parse_checklist_json(&json))
    }

    pub async fn fetch_task_graph(&self) -> Option<TaskGraphSnapshot> {
        let value = self
            .manager
            .get_thread_harness_task_graph(&self.thread.id)
            .await
            .ok()?;
        parse_task_graph_value(&value)
    }

    pub async fn fetch_context_pct(&self) -> Option<u8> {
        let snapshot = self
            .manager
            .get_thread_context(&self.thread.id)
            .await
            .ok()?;
        let pct = snapshot
            .last_api_usage_percent
            .unwrap_or(snapshot.usage_percent);
        Some(pct.round().clamp(0.0, 100.0) as u8)
    }
}

/// Align TUI with desktop: read keyring secret into config for this process.
fn inject_desktop_api_key(config: &mut crate::config::Config) {
    if config
        .api_key
        .as_ref()
        .is_some_and(|k| !k.trim().is_empty())
    {
        return;
    }
    let secrets = zagens_secrets::Secrets::auto_detect();
    if let Some(key) = secrets.resolve("deepseek") {
        config.api_key = Some(key);
    }
}

async fn resolve_thread(
    manager: &RuntimeThreadManager,
    ctx: &CliContext,
    cli: &Cli,
    last_thread_id: Option<&str>,
) -> Result<ThreadRecord> {
    if let Some(ref id) = cli.resume {
        return resolve_by_prefix(manager, id).await;
    }
    if cli.fresh {
        return create_new_thread(manager, ctx, cli).await;
    }
    if let Some(id) = last_thread_id.filter(|s| !s.trim().is_empty())
        && let Ok(thread) = manager.get_thread(id).await
    {
        let workspace_canon =
            std::fs::canonicalize(&ctx.workspace).unwrap_or_else(|_| ctx.workspace.clone());
        let tw =
            std::fs::canonicalize(&thread.workspace).unwrap_or_else(|_| thread.workspace.clone());
        if tw == workspace_canon {
            return Ok(thread);
        }
    }
    match resolve_latest(manager, &ctx.workspace).await {
        Ok(thread) => Ok(thread),
        Err(err) => {
            if cli.continue_session {
                eprintln!("zagens-tui: {err:#}; starting a new session");
            }
            create_new_thread(manager, ctx, cli).await
        }
    }
}

async fn create_new_thread(
    manager: &RuntimeThreadManager,
    ctx: &CliContext,
    cli: &Cli,
) -> Result<ThreadRecord> {
    let mode = if cli.yolo { "yolo" } else { "agent" };
    manager
        .create_thread(tui_create_thread_request(
            ctx.workspace.clone(),
            mode,
            cli.yolo,
            cli.yolo || ctx.config.allow_shell(),
        ))
        .await
}

/// TUI targets code/agent workflows; office task type is not used.
fn tui_create_thread_request(
    workspace: std::path::PathBuf,
    mode: &str,
    yolo: bool,
    allow_shell: bool,
) -> CreateThreadRequest {
    CreateThreadRequest {
        workspace: Some(workspace),
        mode: Some(mode.to_string()),
        auto_approve: Some(yolo),
        allow_shell: Some(allow_shell),
        trust_mode: Some(yolo),
        task_type: Some(TaskType::Code.as_str().to_string()),
        ..Default::default()
    }
}

/// Resume/continue may load legacy `office` threads — normalize to code for TUI.
async fn ensure_tui_code_task_type(
    manager: &RuntimeThreadManager,
    thread: ThreadRecord,
) -> Result<ThreadRecord> {
    if thread.task_type == TaskType::Code.as_str() {
        return Ok(thread);
    }
    let mut updated = thread;
    updated.task_type = TaskType::Code.as_str().to_string();
    updated.updated_at = Utc::now();
    {
        let store = manager.store.clone();
        let copy = updated.clone();
        tokio::task::spawn_blocking(move || store.save_thread(&copy))
            .await
            .map_err(|e| anyhow!("save thread panicked: {e}"))??;
    }
    {
        let mut active = manager.active.lock().await;
        active.engines.remove(&updated.id);
    }
    Ok(updated)
}

async fn resolve_by_prefix(manager: &RuntimeThreadManager, needle: &str) -> Result<ThreadRecord> {
    let needle = needle.trim();
    if needle.is_empty() {
        bail!("--resume requires a thread id or prefix");
    }
    if let Ok(thread) = manager.get_thread(needle).await {
        return Ok(thread);
    }
    let threads = manager
        .list_threads(ThreadListFilter::ActiveOnly, None)
        .await?;
    let matches: Vec<_> = threads
        .into_iter()
        .filter(|t| t.id.starts_with(needle))
        .collect();
    match matches.len() {
        0 => bail!("no thread matches prefix `{needle}`"),
        1 => Ok(matches.into_iter().next().expect("one match")),
        n => bail!("thread prefix `{needle}` is ambiguous ({n} matches)"),
    }
}

async fn resolve_latest(manager: &RuntimeThreadManager, workspace: &Path) -> Result<ThreadRecord> {
    let threads = manager
        .list_threads(ThreadListFilter::ActiveOnly, None)
        .await?;
    let workspace_canon =
        std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    let mut candidates: Vec<_> = threads
        .into_iter()
        .filter(|t| {
            let tw = std::fs::canonicalize(&t.workspace).unwrap_or_else(|_| t.workspace.clone());
            tw == workspace_canon
        })
        .collect();
    if candidates.is_empty() {
        bail!(
            "no saved session in {}",
            crate::cli::context::display_path(workspace)
        );
    }
    candidates.sort_by_key(|t| std::cmp::Reverse(t.updated_at));
    Ok(candidates.remove(0))
}
