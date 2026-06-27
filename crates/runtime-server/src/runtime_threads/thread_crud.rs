//! Sidecar thread CRUD extensions (Config, engine, symbol index).

use std::fs;
use std::ops::Deref;

use anyhow::{Result, anyhow};
use serde_json::json;
use uuid::Uuid;

use super::persist::reconstruct_messages_for_store;
use super::worktree_bind::{maybe_prune_thread_worktree, resolve_thread_workspace_binding};
use super::*;

impl RuntimeThreadManager {
    pub async fn create_thread(&self, req: CreateThreadRequest) -> Result<ThreadRecord> {
        let now = Utc::now();
        let model = req
            .model
            .filter(|m| !m.trim().is_empty())
            .or_else(|| self.config.default_text_model.clone())
            .unwrap_or_else(|| DEFAULT_TEXT_MODEL.to_string());
        let requested_workspace = req.workspace.unwrap_or_else(|| self.workspace.clone());
        let mode = req
            .mode
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| "agent".to_string());
        let allow_shell = req.allow_shell.unwrap_or_else(|| self.config.allow_shell());
        let trust_mode = self.config.effective_trust_mode(req.trust_mode);
        let auto_approve = req.auto_approve.unwrap_or(false);
        let thread_id = format!("thr_{}", &Uuid::new_v4().to_string()[..8]);
        let worktree_cfg = self.config.worktrees_config().runtime_config();
        let binding = resolve_thread_workspace_binding(
            requested_workspace,
            &thread_id,
            req.use_worktree,
            &worktree_cfg,
        )?;
        let task_type =
            crate::task_type::resolve_task_type(req.task_type.as_deref(), &binding.workspace, None);

        let thread = ThreadRecord {
            schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
            id: thread_id,
            created_at: now,
            updated_at: now,
            model,
            workspace: binding.workspace,
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
            scratchpad_run_history: None,
            checklist_snapshot: None,
            plan_snapshot: None,
            config_overlay: None,
            git_root: binding.git_root,
            worktree_name: binding.worktree_name,
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

    pub async fn update_thread(
        &self,
        id: &str,
        mut req: UpdateThreadRequest,
    ) -> Result<ThreadRecord> {
        if let Some(requested) = req.trust_mode {
            req.trust_mode = Some(self.config.effective_trust_mode(Some(requested)));
        }
        let workspace_change = req.workspace.clone();
        let prior_workspace = if workspace_change.is_some() {
            Some(self.load_thread_sync(id)?.workspace)
        } else {
            None
        };
        let prior_thread = if req.archived == Some(true) {
            Some(self.load_thread_sync(id)?)
        } else {
            None
        };
        let thread = self.deref().update_thread(id, req).await?;
        if let (Some(raw), Some(old_ws)) = (workspace_change.as_ref(), prior_workspace) {
            let new_ws = RuntimeThreadManager::resolve_thread_workspace_path(&self.workspace, raw)?;
            let old_canonical = fs::canonicalize(&old_ws).unwrap_or(old_ws);
            if new_ws != old_canonical {
                let rebuild_ws = thread.workspace.clone();
                tokio::task::spawn_blocking(move || {
                    crate::symbol_index::ensure_symbol_index(&rebuild_ws);
                });
            }
        }
        if thread.archived
            && let Some(prior) = prior_thread
            && !prior.archived
        {
            let cfg = self.config.worktrees_config().runtime_config();
            maybe_prune_thread_worktree(&thread, &cfg);
        }
        Ok(thread)
    }

    /// TUI-aligned context usage + compaction policy for Zagens.
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
        let last_api = last_turn.as_ref().and_then(|t| t.last_request_input_tokens);
        let last_reported = last_turn.and_then(|t| t.usage).map(|u| u.input_tokens);

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

    /// Context Explorer breakdown (P2b) — live engine or store reconstruction.
    pub async fn get_thread_context_breakdown(
        &self,
        id: &str,
    ) -> Result<zagens_core::engine::ContextUsageBreakdown> {
        let thread = self.get_thread(id).await?;
        let compaction = self.config.compaction_runtime_config(&thread.model);
        let thresholds = self.config.context.resolved_thresholds_for(&thread.model);
        let seam_enabled = self.config.context.seam_enabled_for_model(&thread.model);
        let system = thread
            .system_prompt
            .as_ref()
            .map(|s| SystemPrompt::Text(s.clone()));

        {
            let active = self.active.lock().await;
            if let Some(state) = active.engines.get(id) {
                let engine = state.engine.clone();
                drop(active);
                if let Ok(breakdown) = engine.query_context_breakdown().await {
                    return Ok(breakdown);
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
        .map_err(|e| anyhow!("get_thread_context_breakdown panicked: {e}"))??;

        Ok(crate::context_snapshot::build_thread_context_breakdown(
            &thread.model,
            &messages,
            system.as_ref(),
            &compaction,
            Some(&thread.workspace),
            None,
            thresholds,
            seam_enabled,
        ))
    }

    pub async fn resume_thread(&self, id: &str) -> Result<ThreadRecord> {
        let thread = self.get_thread(id).await?;
        self.ensure_engine_loaded(&thread).await?;
        Ok(thread)
    }

    #[allow(dead_code)]
    pub async fn resume_thread_with_agent_rebind(
        &self,
        id: &str,
    ) -> Result<(ThreadRecord, Vec<AgentRebindHint>)> {
        let thread = self.resume_thread(id).await?;
        let events = self.events_since_async(&thread.id, None).await?;
        let hints = collect_agent_rebind_hints(&events);
        Ok((thread, hints))
    }
}
