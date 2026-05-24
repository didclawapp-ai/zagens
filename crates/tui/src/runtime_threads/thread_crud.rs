//! Thread CRUD, fork, resume, and session seeding (R-003 A4.6).

use super::persist::reconstruct_messages_for_store;
use super::routing::save_routing_rules;
use super::*;

impl RuntimeThreadManager {
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
        let path = self.routing_rules_path.clone();
        tokio::task::spawn_blocking(move || save_routing_rules(&path, &rules))
            .await
            .context("routing rules save join")??;
        Ok(())
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
        let events = self.events_since_async(&thread.id, None).await?;
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
}

