//! Thread CRUD, fork, and session seeding (R-003 A4.6).

use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::models::{ContentBlock, Message};

use super::manager::RuntimeThreadManager;
use super::routing::save_routing_rules;
use super::types::*;
use super::{
    RoutingRule, ThreadDetail, ThreadListFilter, UpdateThreadRequest, UsageAggregation,
    UsageGroupBy,
};

pub const SUMMARY_LIMIT: usize = 280;

impl<P, R> RuntimeThreadManager<P, R>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
{
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
            if scratchpad_run_id.trim().is_empty() {
                if thread.scratchpad_run_id.is_some() || thread.scratchpad_run_history.is_some() {
                    thread.scratchpad_run_id = None;
                    thread.scratchpad_run_history = None;
                    changes.insert("scratchpad_run_id".to_string(), json!(null));
                }
            } else {
                let before = thread.scratchpad_run_id.clone();
                thread.record_scratchpad_run(scratchpad_run_id.trim());
                if before != thread.scratchpad_run_id {
                    changes.insert(
                        "scratchpad_run_id".to_string(),
                        json!(thread.scratchpad_run_id),
                    );
                }
            }
        }
        if let Some(workspace_raw) = req.workspace.clone() {
            let new_ws = Self::resolve_thread_workspace_path(&self.workspace, &workspace_raw)?;
            let old_canonical =
                fs::canonicalize(&thread.workspace).unwrap_or_else(|_| thread.workspace.clone());
            if new_ws != old_canonical {
                thread.workspace = new_ws;
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

    pub async fn fork_thread(&self, id: &str) -> Result<ThreadRecord> {
        let source = self.get_thread(id).await?;
        let source_id = source.id.clone();
        let source_id_for_event = source_id.clone();
        let store = self.store.clone();

        let forked = tokio::task::spawn_blocking(move || -> Result<ThreadRecord> {
            let mut forked = source.clone();
            let now = Utc::now();
            forked.id = format!("thr_{}", &Uuid::new_v4().to_string()[..8]);
            forked.created_at = now;
            forked.updated_at = now;
            forked.latest_turn_id = None;
            forked.archived = false;
            store.save_thread(&forked)?;

            let source_turns = store.list_turns_for_thread(&source_id)?;
            for source_turn in source_turns {
                let mut cloned_turn = source_turn.clone();
                cloned_turn.id = format!("turn_{}", &Uuid::new_v4().to_string()[..8]);
                cloned_turn.thread_id = forked.id.clone();
                cloned_turn.item_ids.clear();
                store.save_turn(&cloned_turn)?;

                let items = store.list_items_for_turn(&source_turn.id)?;
                for item in items {
                    let mut cloned_item = item.clone();
                    cloned_item.id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
                    cloned_item.turn_id = cloned_turn.id.clone();
                    store.save_item(&cloned_item)?;
                    cloned_turn.item_ids.push(cloned_item.id.clone());
                }
                store.save_turn(&cloned_turn)?;
                forked.latest_turn_id = Some(cloned_turn.id.clone());
                forked.updated_at = now;
                store.save_thread(&forked)?;
            }

            Ok(forked)
        })
        .await
        .map_err(|e| anyhow!("fork_thread blocking task: {e}"))??;

        self.emit_event(
            &forked.id,
            None,
            None,
            "thread.forked",
            json!({
                "thread": forked,
                "source_thread_id": source_id_for_event,
            }),
        )
        .await?;
        Ok(forked)
    }

    pub async fn fork_at_user_message(
        &self,
        id: &str,
        depth_from_tail: usize,
    ) -> Result<(ThreadRecord, Option<String>)> {
        let source = self.get_thread(id).await?;
        let source_id = source.id.clone();
        let source_id_for_event = source_id.clone();
        let store = self.store.clone();

        let (forked, original_user_text, target_turn_id) = tokio::task::spawn_blocking(
            move || -> Result<(ThreadRecord, Option<String>, String)> {
                let source_turns = store.list_turns_for_thread(&source_id)?;

                let mut user_turn_indices: Vec<usize> = Vec::new();
                for (idx, turn) in source_turns.iter().enumerate().rev() {
                    let items = store.list_items_for_turn(&turn.id)?;
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
                let target_turn_idx = user_turn_indices[depth_from_tail];
                let target_turn_id = source_turns[target_turn_idx].id.clone();

                let target_items = store.list_items_for_turn(&target_turn_id)?;
                let original_user_text = target_items
                    .iter()
                    .find(|item| item.kind == TurnItemKind::UserMessage)
                    .and_then(|item| item.detail.clone());

                let mut forked = source.clone();
                let now = Utc::now();
                forked.id = format!("thr_{}", &Uuid::new_v4().to_string()[..8]);
                forked.created_at = now;
                forked.updated_at = now;
                forked.latest_turn_id = None;
                forked.archived = false;
                store.save_thread(&forked)?;

                for source_turn in source_turns.iter().take(target_turn_idx) {
                    let mut cloned_turn = source_turn.clone();
                    cloned_turn.id = format!("turn_{}", &Uuid::new_v4().to_string()[..8]);
                    cloned_turn.thread_id = forked.id.clone();
                    cloned_turn.item_ids.clear();
                    store.save_turn(&cloned_turn)?;

                    let items = store.list_items_for_turn(&source_turn.id)?;
                    for item in items {
                        let mut cloned_item = item.clone();
                        cloned_item.id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
                        cloned_item.turn_id = cloned_turn.id.clone();
                        store.save_item(&cloned_item)?;
                        cloned_turn.item_ids.push(cloned_item.id.clone());
                    }
                    store.save_turn(&cloned_turn)?;
                    forked.latest_turn_id = Some(cloned_turn.id.clone());
                    forked.updated_at = now;
                    store.save_thread(&forked)?;
                }

                Ok((forked, original_user_text, target_turn_id))
            },
        )
        .await
        .map_err(|e| anyhow!("fork_at_user_message blocking task: {e}"))??;

        self.emit_event(
            &forked.id,
            None,
            None,
            "thread.forked",
            json!({
                "thread": forked,
                "source_thread_id": source_id_for_event,
                "backtrack_depth_from_tail": depth_from_tail,
                "dropped_turn_id": target_turn_id,
            }),
        )
        .await?;
        Ok((forked, original_user_text))
    }

    pub async fn seed_thread_from_messages(
        &self,
        thread_id: &str,
        messages: &[Message],
    ) -> Result<()> {
        let mut thread = self.get_thread(thread_id).await?;
        let now = Utc::now();

        let mut turns: Vec<(String, Vec<ContentBlock>)> = Vec::new();
        let mut idx = 0usize;
        while idx < messages.len() {
            if messages[idx].role != "user" {
                idx += 1;
                continue;
            }
            let user_text = messages[idx]
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            idx += 1;
            let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
            while idx < messages.len() && messages[idx].role == "assistant" {
                assistant_blocks.extend(messages[idx].content.clone());
                idx += 1;
            }
            if user_text.trim().is_empty() && assistant_blocks.is_empty() {
                continue;
            }
            turns.push((user_text, assistant_blocks));
        }

        for (user_text, assistant_blocks) in turns {
            let turn_id = format!("turn_{}", &Uuid::new_v4().to_string()[..8]);
            let summary = super::summarize_text(&user_text, SUMMARY_LIMIT);
            let mut item_ids = Vec::new();

            if !user_text.trim().is_empty() {
                let item = Self::seed_user_item(&turn_id, &user_text, now);
                let item_id = item.id.clone();
                self.store.save_item(&item)?;
                self.emit_seed_item_completed(thread_id, &turn_id, &item, None)
                    .await?;
                item_ids.push(item_id);
            }

            let mut block_idx = 0usize;
            while block_idx < assistant_blocks.len() {
                match &assistant_blocks[block_idx] {
                    ContentBlock::Text { text, .. } => {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            let item = Self::seed_agent_item(&turn_id, trimmed, now);
                            let item_id = item.id.clone();
                            self.store.save_item(&item)?;
                            self.emit_seed_item_completed(thread_id, &turn_id, &item, None)
                                .await?;
                            item_ids.push(item_id);
                        }
                        block_idx += 1;
                    }
                    ContentBlock::ToolUse {
                        id, name, input, ..
                    } => {
                        let tool_result = assistant_blocks.get(block_idx + 1);
                        let (output, is_error) = match tool_result {
                            Some(ContentBlock::ToolResult {
                                content, is_error, ..
                            }) => (content.clone(), is_error.unwrap_or(false)),
                            _ => (String::new(), false),
                        };
                        let item =
                            Self::seed_tool_item(&turn_id, id, name, input, &output, is_error, now);
                        let item_id = item.id.clone();
                        self.store.save_item(&item)?;
                        let tool_payload = json!({
                            "id": id,
                            "name": name,
                            "input": input,
                        });
                        self.emit_seed_item_started(thread_id, &turn_id, &item, &tool_payload)
                            .await?;
                        self.emit_seed_item_completed(
                            thread_id,
                            &turn_id,
                            &item,
                            Some(tool_payload),
                        )
                        .await?;
                        item_ids.push(item_id);
                        block_idx += if tool_result.is_some() { 2 } else { 1 };
                    }
                    ContentBlock::ToolResult { .. } => {
                        block_idx += 1;
                    }
                    _ => {
                        block_idx += 1;
                    }
                }
            }

            self.store.save_turn(&TurnRecord {
                schema_version: super::CURRENT_RUNTIME_SCHEMA_VERSION,
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

    fn seed_user_item(turn_id: &str, text: &str, now: DateTime<Utc>) -> TurnItemRecord {
        let item_id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
        TurnItemRecord {
            schema_version: super::CURRENT_RUNTIME_SCHEMA_VERSION,
            id: item_id,
            turn_id: turn_id.to_string(),
            kind: TurnItemKind::UserMessage,
            status: TurnItemLifecycleStatus::Completed,
            summary: super::summarize_text(text, SUMMARY_LIMIT),
            detail: Some(text.to_string()),
            metadata: None,
            artifact_refs: Vec::new(),
            started_at: Some(now),
            ended_at: Some(now),
        }
    }

    fn seed_agent_item(turn_id: &str, text: &str, now: DateTime<Utc>) -> TurnItemRecord {
        let item_id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
        TurnItemRecord {
            schema_version: super::CURRENT_RUNTIME_SCHEMA_VERSION,
            id: item_id,
            turn_id: turn_id.to_string(),
            kind: TurnItemKind::AgentMessage,
            status: TurnItemLifecycleStatus::Completed,
            summary: super::summarize_text(text, SUMMARY_LIMIT),
            detail: Some(text.to_string()),
            metadata: None,
            artifact_refs: Vec::new(),
            started_at: Some(now),
            ended_at: Some(now),
        }
    }

    fn seed_tool_item(
        turn_id: &str,
        tool_use_id: &str,
        name: &str,
        input: &Value,
        output: &str,
        is_error: bool,
        now: DateTime<Utc>,
    ) -> TurnItemRecord {
        let item_id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
        let status = if is_error {
            TurnItemLifecycleStatus::Failed
        } else {
            TurnItemLifecycleStatus::Completed
        };
        TurnItemRecord {
            schema_version: super::CURRENT_RUNTIME_SCHEMA_VERSION,
            id: item_id,
            turn_id: turn_id.to_string(),
            kind: super::session_reconstruct::tool_item_kind_for_seed(name),
            status,
            summary: super::summarize_text(&format!("{name}: {output}"), SUMMARY_LIMIT),
            detail: Some(output.to_string()),
            metadata: Some(json!({
                "tool_name": name,
                "tool_input": input,
                "engine_tool_id": tool_use_id,
            })),
            artifact_refs: Vec::new(),
            started_at: Some(now),
            ended_at: Some(now),
        }
    }

    fn seed_item_kind_str(kind: TurnItemKind) -> &'static str {
        match kind {
            TurnItemKind::UserMessage => "user_message",
            TurnItemKind::AgentMessage => "agent_message",
            TurnItemKind::ToolCall => "tool_call",
            TurnItemKind::FileChange => "file_change",
            TurnItemKind::CommandExecution => "command_execution",
            TurnItemKind::ContextCompaction => "context_compaction",
            TurnItemKind::Status => "status",
            TurnItemKind::Error => "error",
        }
    }

    async fn emit_seed_item_started(
        &self,
        thread_id: &str,
        turn_id: &str,
        item: &TurnItemRecord,
        tool: &Value,
    ) -> Result<()> {
        let _ = self
            .emit_event(
                thread_id,
                Some(turn_id),
                Some(&item.id),
                "item.started",
                json!({ "item": item, "tool": tool }),
            )
            .await?;
        Ok(())
    }

    async fn emit_seed_item_completed(
        &self,
        thread_id: &str,
        turn_id: &str,
        item: &TurnItemRecord,
        tool: Option<Value>,
    ) -> Result<()> {
        let kind = Self::seed_item_kind_str(item.kind);
        let mut payload = json!({
            "item": {
                "id": item.id,
                "kind": kind,
                "detail": item.detail,
                "summary": item.summary,
            }
        });
        if let Some(tool) = tool {
            payload["tool"] = tool;
        }
        let event = if item.status == TurnItemLifecycleStatus::Failed {
            "item.failed"
        } else {
            "item.completed"
        };
        let _ = self
            .emit_event(thread_id, Some(turn_id), Some(&item.id), event, payload)
            .await?;
        Ok(())
    }
}
