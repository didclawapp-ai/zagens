//! Thread/turn/item disk store and usage aggregation (R-003 A4.6).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::models::{ContentBlock, Message};
use crate::thread_store_sqlite;

use super::types::*;
use super::{
    provider_label_for_model, UsageAggregation, UsageBucket, UsageGroupBy, UsageTotals,
    CURRENT_RUNTIME_SCHEMA_VERSION,
};

#[derive(Debug, Clone)]
pub struct RuntimeThreadStore {
    pub(crate) threads_dir: PathBuf,
    pub(crate) turns_dir: PathBuf,
    pub(crate) items_dir: PathBuf,
    pub(crate) events_dir: PathBuf,
    pub(crate) state_path: PathBuf,
    pub(crate) state: Arc<Mutex<RuntimeStoreState>>,
    pub(crate) db: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,
}

impl RuntimeThreadStore {
    fn prepare_dirs(root: &Path) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf, PathBuf)> {
        let threads_dir = root.join("threads");
        let turns_dir = root.join("turns");
        let items_dir = root.join("items");
        let events_dir = root.join("events");
        fs::create_dir_all(&threads_dir)
            .with_context(|| format!("Failed to create {}", threads_dir.display()))?;
        fs::create_dir_all(&turns_dir)
            .with_context(|| format!("Failed to create {}", turns_dir.display()))?;
        fs::create_dir_all(&items_dir)
            .with_context(|| format!("Failed to create {}", items_dir.display()))?;
        fs::create_dir_all(&events_dir)
            .with_context(|| format!("Failed to create {}", events_dir.display()))?;
        Ok((
            threads_dir,
            turns_dir,
            items_dir,
            events_dir,
            root.join("state.json"),
        ))
    }

    /// JSON-per-file store only (tests that write fixture JSON under `threads/`).
    #[cfg(test)]
    pub fn open_json_only(root: PathBuf) -> Result<Self> {
        let (threads_dir, turns_dir, items_dir, events_dir, state_path) = Self::prepare_dirs(&root)?;
        let state = if state_path.exists() {
            let raw = fs::read_to_string(&state_path)
                .with_context(|| format!("Failed to read {}", state_path.display()))?;
            serde_json::from_str::<RuntimeStoreState>(&raw)
                .with_context(|| format!("Failed to parse {}", state_path.display()))?
        } else {
            let default = RuntimeStoreState::default();
            write_json_atomic(&state_path, &default)?;
            default
        };
        Ok(Self {
            threads_dir,
            turns_dir,
            items_dir,
            events_dir,
            state_path,
            state: Arc::new(Mutex::new(state)),
            db: None,
        })
    }

    pub fn open(root: PathBuf) -> Result<Self> {
        let (threads_dir, turns_dir, items_dir, events_dir, state_path) = Self::prepare_dirs(&root)?;

        // SQLite backend with auto-migration from existing JSON files.
        let db_path = root.join("runtime.db");
        let (state, db) = match crate::thread_store_sqlite::open_sqlite_thread_db(&db_path, &threads_dir) {
            Ok((conn, sqlite_state)) => {
                eprintln!("[thread-store] SQLite backend active at {}", db_path.display());
                (sqlite_state, Some(Arc::new(std::sync::Mutex::new(conn))))
            }
            Err(e) => {
                eprintln!("[thread-store] SQLite unavailable ({}): falling back to JSON files", e);
                let state = if state_path.exists() {
                    let raw = fs::read_to_string(&state_path)
                        .with_context(|| format!("Failed to read {}", state_path.display()))?;
                    serde_json::from_str::<RuntimeStoreState>(&raw)
                        .with_context(|| format!("Failed to parse {}", state_path.display()))?
                } else {
                    let default = RuntimeStoreState::default();
                    write_json_atomic(&state_path, &default)?;
                    default
                };
                (state, None)
            }
        };

        Ok(Self {
            threads_dir,
            turns_dir,
            items_dir,
            events_dir,
            state_path,
            state: Arc::new(Mutex::new(state)),
            db,
        })
    }

    fn thread_path(&self, thread_id: &str) -> PathBuf {
        self.threads_dir.join(format!("{thread_id}.json"))
    }

    fn turn_path(&self, turn_id: &str) -> PathBuf {
        self.turns_dir.join(format!("{turn_id}.json"))
    }

    fn item_path(&self, item_id: &str) -> PathBuf {
        self.items_dir.join(format!("{item_id}.json"))
    }

    fn events_path(&self, thread_id: &str) -> PathBuf {
        self.events_dir.join(format!("{thread_id}.jsonl"))
    }

    pub fn save_thread(&self, thread: &ThreadRecord) -> Result<()> {
        if let Some(ref db) = self.db {
            return crate::thread_store_sqlite::save_thread_sqlite(&db.lock().unwrap(), thread)
                .map_err(|e| anyhow!("save_thread sqlite: {e}"));
        }
        write_json_atomic(&self.thread_path(&thread.id), thread)
    }

    pub fn save_turn(&self, turn: &TurnRecord) -> Result<()> {
        if let Some(ref db) = self.db {
            return crate::thread_store_sqlite::save_turn_sqlite(&db.lock().unwrap(), turn)
                .map_err(|e| anyhow!("save_turn sqlite: {e}"));
        }
        write_json_atomic(&self.turn_path(&turn.id), turn)
    }

    pub fn save_item(&self, item: &TurnItemRecord) -> Result<()> {
        if let Some(ref db) = self.db {
            return crate::thread_store_sqlite::save_item_sqlite(&db.lock().unwrap(), item)
                .map_err(|e| anyhow!("save_item sqlite: {e}"));
        }
        write_json_atomic(&self.item_path(&item.id), item)
    }

    pub fn load_thread(&self, thread_id: &str) -> Result<ThreadRecord> {
        if let Some(ref db) = self.db {
            return crate::thread_store_sqlite::load_thread_sqlite(&db.lock().unwrap(), thread_id)
                .map_err(|e| anyhow!("{e}"));
        }
        let path = self.thread_path(thread_id);
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read thread {}", path.display()))?;
        let record: ThreadRecord = serde_json::from_str(&raw)
            .with_context(|| format!("Failed to parse thread {}", path.display()))?;
        if record.schema_version > CURRENT_RUNTIME_SCHEMA_VERSION {
            bail!(
                "Thread schema v{} is newer than supported v{}",
                record.schema_version,
                CURRENT_RUNTIME_SCHEMA_VERSION
            );
        }
        Ok(record)
    }

    pub fn load_turn(&self, turn_id: &str) -> Result<TurnRecord> {
        if let Some(ref db) = self.db {
            return crate::thread_store_sqlite::load_turn_sqlite(&db.lock().unwrap(), turn_id)
                .map_err(|e| anyhow!("{e}"));
        }
        let path = self.turn_path(turn_id);
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read turn {}", path.display()))?;
        let record: TurnRecord = serde_json::from_str(&raw)
            .with_context(|| format!("Failed to parse turn {}", path.display()))?;
        if record.schema_version > CURRENT_RUNTIME_SCHEMA_VERSION {
            bail!(
                "Turn schema v{} is newer than supported v{}",
                record.schema_version,
                CURRENT_RUNTIME_SCHEMA_VERSION
            );
        }
        Ok(record)
    }

    pub fn load_item(&self, item_id: &str) -> Result<TurnItemRecord> {
        if let Some(ref db) = self.db {
            return crate::thread_store_sqlite::load_item_sqlite(&db.lock().unwrap(), item_id)
                .map_err(|e| anyhow!("load_item: {e}"));
        }
        let path = self.item_path(item_id);
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read item {}", path.display()))?;
        let record: TurnItemRecord = serde_json::from_str(&raw)
            .with_context(|| format!("Failed to parse item {}", path.display()))?;
        if record.schema_version > CURRENT_RUNTIME_SCHEMA_VERSION {
            bail!(
                "Item schema v{} is newer than supported v{}",
                record.schema_version,
                CURRENT_RUNTIME_SCHEMA_VERSION
            );
        }
        Ok(record)
    }

    pub fn list_threads(&self) -> Result<Vec<ThreadRecord>> {
        if let Some(ref db) = self.db {
            return crate::thread_store_sqlite::list_threads_sqlite(&db.lock().unwrap())
                .map_err(|e| anyhow!("list_threads: {e}"));
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.threads_dir)
            .with_context(|| format!("Failed to read {}", self.threads_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let thread: ThreadRecord = serde_json::from_str(&raw)
                .with_context(|| format!("Failed to parse {}", path.display()))?;
            if thread.schema_version > CURRENT_RUNTIME_SCHEMA_VERSION {
                bail!(
                    "Thread schema v{} is newer than supported v{}",
                    thread.schema_version,
                    CURRENT_RUNTIME_SCHEMA_VERSION
                );
            }
            out.push(thread);
        }
        out.sort_by_key(|t| std::cmp::Reverse(t.updated_at));
        Ok(out)
    }

    pub fn list_turns_for_thread(&self, thread_id: &str) -> Result<Vec<TurnRecord>> {
        if let Some(ref db) = self.db {
            return crate::thread_store_sqlite::list_turns_for_thread_sqlite(&db.lock().unwrap(), thread_id)
                .map_err(|e| anyhow!("list_turns_for_thread: {e}"));
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.turns_dir)
            .with_context(|| format!("Failed to read {}", self.turns_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let turn: TurnRecord = serde_json::from_str(&raw)
                .with_context(|| format!("Failed to parse {}", path.display()))?;
            if turn.schema_version > CURRENT_RUNTIME_SCHEMA_VERSION {
                bail!(
                    "Turn schema v{} is newer than supported v{}",
                    turn.schema_version,
                    CURRENT_RUNTIME_SCHEMA_VERSION
                );
            }
            if turn.thread_id == thread_id {
                out.push(turn);
            }
        }
        out.sort_by_key(|a| a.created_at);
        Ok(out)
    }

    /// One linear scan of `turns_dir` for startup recovery (avoids O(threads ×
    /// turns) when `list_turns_for_thread` is called per thread).
    pub fn list_incomplete_turns(&self) -> Result<Vec<TurnRecord>> {
        if let Some(ref db) = self.db {
            return crate::thread_store_sqlite::list_incomplete_turns_sqlite(&db.lock().unwrap())
                .map_err(|e| anyhow!("list_incomplete_turns: {e}"));
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.turns_dir)
            .with_context(|| format!("Failed to read {}", self.turns_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let turn: TurnRecord = serde_json::from_str(&raw)
                .with_context(|| format!("Failed to parse {}", path.display()))?;
            if turn.schema_version > CURRENT_RUNTIME_SCHEMA_VERSION {
                bail!(
                    "Turn schema v{} is newer than supported v{}",
                    turn.schema_version,
                    CURRENT_RUNTIME_SCHEMA_VERSION
                );
            }
            if matches!(
                turn.status,
                RuntimeTurnStatus::Queued | RuntimeTurnStatus::InProgress
            ) {
                out.push(turn);
            }
        }
        Ok(out)
    }

    pub fn list_items_for_turn(&self, turn_id: &str) -> Result<Vec<TurnItemRecord>> {
        if let Some(ref db) = self.db {
            return crate::thread_store_sqlite::list_items_for_turn_sqlite(&db.lock().unwrap(), turn_id)
                .map_err(|e| anyhow!("list_items_for_turn: {e}"));
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.items_dir)
            .with_context(|| format!("Failed to read {}", self.items_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let item: TurnItemRecord = serde_json::from_str(&raw)
                .with_context(|| format!("Failed to parse {}", path.display()))?;
            if item.schema_version > CURRENT_RUNTIME_SCHEMA_VERSION {
                bail!(
                    "Item schema v{} is newer than supported v{}",
                    item.schema_version,
                    CURRENT_RUNTIME_SCHEMA_VERSION
                );
            }
            if item.turn_id == turn_id {
                out.push(item);
            }
        }
        out.sort_by(|a, b| {
            let left = a.started_at.unwrap_or_else(Utc::now);
            let right = b.started_at.unwrap_or_else(Utc::now);
            left.cmp(&right)
        });
        Ok(out)
    }

    pub async fn append_event(
        &self,
        thread_id: &str,
        turn_id: Option<&str>,
        item_id: Option<&str>,
        event: impl Into<String>,
        payload: Value,
    ) -> Result<RuntimeEventRecord> {
        let mut state = self.state.lock().await;
        let seq = state.next_seq;
        state.next_seq = state.next_seq.saturating_add(1);

        let record = RuntimeEventRecord {
            schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
            seq,
            timestamp: Utc::now(),
            thread_id: thread_id.to_string(),
            turn_id: turn_id.map(ToString::to_string),
            item_id: item_id.map(ToString::to_string),
            event: event.into(),
            payload,
        };

        if let Some(ref db) = self.db {
            let db = Arc::clone(db);
            let record_for_db = record.clone();
            drop(state);
            tokio::task::spawn_blocking(move || {
                crate::thread_store_sqlite::append_event_sqlite(
                    &db.lock().unwrap(),
                    &record_for_db,
                    seq,
                )
            })
            .await
            .map_err(|e| anyhow!("append_event join: {e}"))?
            .map_err(|e| anyhow!("append_event sqlite: {e}"))?;
            return Ok(record);
        }

        let state_path = self.state_path.clone();
        let state_snapshot = state.clone();
        let events_path = self.events_path(thread_id);
        let record_for_disk = record.clone();
        drop(state);

        tokio::task::spawn_blocking(move || {
            write_json_atomic(&state_path, &state_snapshot)?;
            append_event_jsonl_blocking(&events_path, &record_for_disk)
        })
        .await
        .map_err(|e| anyhow!("append_event join: {e}"))??;

        Ok(record)
    }

    pub fn events_since(
        &self,
        thread_id: &str,
        since_seq: Option<u64>,
    ) -> Result<Vec<RuntimeEventRecord>> {
        if let Some(ref db) = self.db {
            let since = since_seq.unwrap_or(0);
            return crate::thread_store_sqlite::events_since_sqlite(&db.lock().unwrap(), thread_id, since)
                .map_err(|e| anyhow!("events_since: {e}"));
        }
        let path = self.events_path(thread_id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file =
            File::open(&path).with_context(|| format!("Failed to open {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut out = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let event: RuntimeEventRecord = serde_json::from_str(&line)
                .with_context(|| format!("Failed to parse event line in {}", path.display()))?;
            if let Some(since) = since_seq
                && event.seq <= since
            {
                continue;
            }
            out.push(event);
        }
        Ok(out)
    }

    pub async fn current_seq(&self) -> u64 {
        let state = self.state.lock().await;
        state.next_seq.saturating_sub(1)
    }
}

impl RuntimeThreadStore {
    pub fn aggregate_usage_linear(
        &self,
        thread_models: &HashMap<String, String>,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
        group_by: UsageGroupBy,
    ) -> Result<UsageAggregation> {
        if let Some(ref db) = self.db {
            return crate::thread_store_sqlite::aggregate_usage_linear_sqlite(
                &db.lock().unwrap(),
                since,
                until,
                group_by,
            )
            .map_err(|e| anyhow!("aggregate_usage sqlite: {e}"));
        }
        let mut buckets: BTreeMap<String, UsageBucket> = BTreeMap::new();
        let mut totals = UsageTotals::default();

        for entry in fs::read_dir(&self.turns_dir)
            .with_context(|| format!("Failed to read {}", self.turns_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let turn: TurnRecord = serde_json::from_str(&raw)
                .with_context(|| format!("Failed to parse {}", path.display()))?;
            if turn.schema_version > CURRENT_RUNTIME_SCHEMA_VERSION {
                bail!(
                    "Turn schema v{} is newer than supported v{}",
                    turn.schema_version,
                    CURRENT_RUNTIME_SCHEMA_VERSION
                );
            }
            let Some(model) = thread_models.get(&turn.thread_id) else {
                continue;
            };
            if let Some(s) = since
                && turn.created_at < s
            {
                continue;
            }
            if let Some(u) = until
                && turn.created_at > u
            {
                continue;
            }
            let Some(usage) = turn.usage.as_ref() else {
                continue;
            };
            let cached = usage.prompt_cache_hit_tokens.unwrap_or(0) as u64;
            let reasoning = usage.reasoning_tokens.unwrap_or(0) as u64;
            let input = usage.input_tokens as u64;
            let output = usage.output_tokens as u64;
            let cost = crate::pricing::calculate_turn_cost_from_usage(model, usage).unwrap_or(0.0);

            totals.input_tokens += input;
            totals.output_tokens += output;
            totals.cached_tokens += cached;
            totals.reasoning_tokens += reasoning;
            totals.cost_usd += cost;
            totals.turns += 1;

            let key = match group_by {
                UsageGroupBy::Day => turn.created_at.format("%Y-%m-%d").to_string(),
                UsageGroupBy::Model => model.clone(),
                UsageGroupBy::Provider => provider_label_for_model(model).to_string(),
                UsageGroupBy::Thread => turn.thread_id.clone(),
            };
            let bucket = buckets.entry(key.clone()).or_insert_with(|| UsageBucket {
                key,
                ..UsageBucket::default()
            });
            bucket.input_tokens += input;
            bucket.output_tokens += output;
            bucket.cached_tokens += cached;
            bucket.reasoning_tokens += reasoning;
            bucket.cost_usd += cost;
            bucket.turns += 1;
        }

        let group_by_str = match group_by {
            UsageGroupBy::Day => "day",
            UsageGroupBy::Model => "model",
            UsageGroupBy::Provider => "provider",
            UsageGroupBy::Thread => "thread",
        }
        .to_string();

        Ok(UsageAggregation {
            since,
            until,
            group_by: group_by_str,
            totals,
            buckets: buckets.into_values().collect(),
        })
    }
}

pub(super) fn duration_ms(start: DateTime<Utc>, end: DateTime<Utc>) -> u64 {
    let millis = (end - start).num_milliseconds();
    if millis.is_negative() {
        0
    } else {
        u64::try_from(millis).unwrap_or(u64::MAX)
    }
}

/// Standalone version of `reconstruct_messages_from_turns` that works on a
/// cloned `RuntimeThreadStore` — so it can run inside `spawn_blocking` without
/// holding a reference to the `RuntimeThreadManager` or its `Arc<Mutex<…>>` locks.
pub(super) fn reconstruct_messages_for_store(
    store: &RuntimeThreadStore,
    turns: &[TurnRecord],
) -> Result<Vec<Message>> {
    let mut messages = Vec::new();
    for turn in turns {
        let items = store.list_items_for_turn(&turn.id)?;
        for item in items {
            match item.kind {
                TurnItemKind::UserMessage => {
                    let text = item.detail.unwrap_or(item.summary);
                    messages.push(Message {
                        role: "user".to_string(),
                        content: vec![ContentBlock::Text {
                            text,
                            cache_control: None,
                        }],
                    });
                }
                TurnItemKind::AgentMessage => {
                    let text = item.detail.unwrap_or(item.summary);
                    messages.push(Message {
                        role: "assistant".to_string(),
                        content: vec![ContentBlock::Text {
                            text,
                            cache_control: None,
                        }],
                    });
                }
                _ => {}
            }
        }
    }
    Ok(messages)
}

fn append_event_jsonl_blocking(path: &Path, record: &RuntimeEventRecord) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("Failed to open {}", path.display()))?;
    let line = serde_json::to_string(record)?;
    writeln!(file, "{line}").with_context(|| format!("Failed to append {}", path.display()))?;
    file.flush()
        .with_context(|| format!("Failed to flush {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("Failed to fsync {}", path.display()))?;
    Ok(())
}

pub(super) fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let payload = serde_json::to_string_pretty(value)?;
    crate::utils::write_atomic(path, payload.as_bytes())
        .with_context(|| format!("Failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_threads::types::{
        RuntimeEventRecord, TurnItemKind, TurnItemLifecycleStatus, TurnItemRecord, TurnRecord,
    };
    use crate::runtime_threads::CURRENT_EVENT_SCHEMA_VERSION;
    use crate::runtime_threads::RuntimeTurnStatus;
    use chrono::Utc;
    use serde_json::json;

    fn temp_store() -> (tempfile::TempDir, RuntimeThreadStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = RuntimeThreadStore::open_json_only(dir.path().to_path_buf()).expect("open store");
        (dir, store)
    }

    fn user_item(turn_id: &str, item_id: &str, text: &str) -> TurnItemRecord {
        TurnItemRecord {
            schema_version: CURRENT_EVENT_SCHEMA_VERSION,
            id: item_id.to_string(),
            turn_id: turn_id.to_string(),
            kind: TurnItemKind::UserMessage,
            status: TurnItemLifecycleStatus::Completed,
            summary: text.to_string(),
            detail: Some(text.to_string()),
            metadata: None,
            artifact_refs: Vec::new(),
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
        }
    }

    fn agent_item(turn_id: &str, item_id: &str, text: &str) -> TurnItemRecord {
        TurnItemRecord {
            schema_version: CURRENT_EVENT_SCHEMA_VERSION,
            id: item_id.to_string(),
            turn_id: turn_id.to_string(),
            kind: TurnItemKind::AgentMessage,
            status: TurnItemLifecycleStatus::Completed,
            summary: text.to_string(),
            detail: Some(text.to_string()),
            metadata: None,
            artifact_refs: Vec::new(),
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
        }
    }

    fn message_visible_text(message: &Message) -> String {
        message
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    fn jsonl_item_detail(event: &RuntimeEventRecord) -> Option<String> {
        if event.event != "item.completed" {
            return None;
        }
        let kind = event.payload.get("item")?.get("kind")?.as_str()?;
        if kind != "user_message" && kind != "agent_message" {
            return None;
        }
        event
            .payload
            .get("item")?
            .get("detail")?
            .as_str()
            .map(str::to_string)
    }

    /// A1.4 — persisted turn items, JSONL `item.completed`, and `reconstruct_messages` agree.
    #[tokio::test]
    async fn reconstruct_messages_matches_jsonl_item_completed_details() {
        let (_dir, store) = temp_store();
        let thread_id = "thr_iso";
        let turn_id = "turn_iso";

        let user = user_item(turn_id, "item_user", "hello from user");
        let agent = agent_item(turn_id, "item_agent", "hello from agent");
        store.save_item(&user).expect("save user item");
        store.save_item(&agent).expect("save agent item");

        let turn = TurnRecord {
            schema_version: CURRENT_EVENT_SCHEMA_VERSION,
            id: turn_id.to_string(),
            thread_id: thread_id.to_string(),
            status: RuntimeTurnStatus::Completed,
            input_summary: "hello from user".to_string(),
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
            duration_ms: Some(1),
            usage: None,
            last_request_input_tokens: None,
            error: None,
            item_ids: vec![user.id.clone(), agent.id.clone()],
            steer_count: 0,
        };
        store.save_turn(&turn).expect("save turn");

        for item in [&user, &agent] {
            let kind = match item.kind {
                TurnItemKind::UserMessage => "user_message",
                TurnItemKind::AgentMessage => "agent_message",
                _ => "status",
            };
            store
                .append_event(
                    thread_id,
                    Some(turn_id),
                    Some(item.id.as_str()),
                    "item.completed",
                    json!({
                        "item": {
                            "id": item.id,
                            "kind": kind,
                            "detail": item.detail,
                        }
                    }),
                )
                .await
                .expect("append jsonl event");
        }

        let turns = store.list_turns_for_thread(thread_id).expect("list turns");
        let reconstructed = reconstruct_messages_for_store(&store, &turns).expect("reconstruct");
        let reconstructed_texts: Vec<String> = reconstructed.iter().map(message_visible_text).collect();

        let events = store.events_since(thread_id, None).expect("read jsonl");
        let jsonl_texts: Vec<String> = events.iter().filter_map(jsonl_item_detail).collect();

        assert_eq!(
            reconstructed_texts,
            vec![
                "hello from user".to_string(),
                "hello from agent".to_string()
            ]
        );
        assert_eq!(jsonl_texts, reconstructed_texts);
        assert!(
            crate::tui::history_isomorphism::history_transcript_core_matches_messages(
                &reconstructed
            ),
            "reconstructed messages must round-trip through TUI history rebuild"
        );
    }

    /// A1.2 — large-output synthesis in turn item `detail` matches JSONL and on-disk blob.
    #[tokio::test]
    async fn large_output_tool_item_detail_matches_jsonl_and_persisted_blob() {
        use crate::tools::large_output_router::{
            artifact_refs_from_tool_output, large_output_dir, load_raw_from_artifact_meta_path,
            LargeOutputExternalRef, LargeOutputRouter, load_large_output_persist_record,
            parse_workshop_ref_from_message, persist_large_output_blob,
        };

        let (_dir, store) = temp_store();
        let thread_id = "thr_lout";
        let turn_id = "turn_lout";
        let session_id = "sess_lout_iso";

        let raw = "line\n".repeat(1200);
        let external_ref = LargeOutputExternalRef::new("read_file", raw.chars().count());
        persist_large_output_blob(session_id, &external_ref, &raw).expect("persist blob");

        let wrapped = LargeOutputRouter::wrap_synthesis(
            "read_file",
            "condensed preview",
            5000,
            4096,
            Some(&external_ref),
        );

        let meta_path =
            large_output_dir(session_id).join(format!("{}.json", external_ref.ref_id));
        let tool_metadata = serde_json::json!({
            crate::tools::large_output_router::LARGE_OUTPUT_METADATA_KEY: {
                "ref_id": external_ref.ref_id,
                "meta_path": meta_path.display().to_string(),
            }
        });
        let artifact_refs = artifact_refs_from_tool_output(
            Some(session_id),
            &wrapped,
            Some(&tool_metadata),
        );
        assert_eq!(artifact_refs.len(), 1, "monitor path must resolve meta ref");

        let tool_item = TurnItemRecord {
            schema_version: CURRENT_EVENT_SCHEMA_VERSION,
            id: "item_tool".to_string(),
            turn_id: turn_id.to_string(),
            kind: TurnItemKind::ToolCall,
            status: TurnItemLifecycleStatus::Completed,
            summary: "read_file: condensed".to_string(),
            detail: Some(wrapped.clone()),
            metadata: Some(tool_metadata),
            artifact_refs,
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
        };
        store.save_item(&tool_item).expect("save tool item");

        let turn = TurnRecord {
            schema_version: CURRENT_EVENT_SCHEMA_VERSION,
            id: turn_id.to_string(),
            thread_id: thread_id.to_string(),
            status: RuntimeTurnStatus::Completed,
            input_summary: "run read".to_string(),
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
            duration_ms: Some(1),
            usage: None,
            last_request_input_tokens: None,
            error: None,
            item_ids: vec![tool_item.id.clone()],
            steer_count: 0,
        };
        store.save_turn(&turn).expect("save turn");

        store
            .append_event(
                thread_id,
                Some(turn_id),
                Some(tool_item.id.as_str()),
                "item.completed",
                json!({ "item": tool_item }),
            )
            .await
            .expect("append jsonl");

        let events = store.events_since(thread_id, None).expect("events");
        let jsonl_detail = events
            .iter()
            .find(|ev| ev.event == "item.completed")
            .and_then(|ev| ev.payload.get("item"))
            .and_then(|item| item.get("detail"))
            .and_then(|d| d.as_str())
            .expect("jsonl item detail");

        assert_eq!(jsonl_detail, wrapped);

        let loaded = store.load_item(&tool_item.id).expect("load item");
        assert_eq!(loaded.detail.as_deref(), Some(wrapped.as_str()));

        let parsed = parse_workshop_ref_from_message(jsonl_detail).expect("workshop-ref");
        assert_eq!(parsed.ref_id, external_ref.ref_id);

        let record =
            load_large_output_persist_record(session_id, &parsed.ref_id).expect("persist record");
        assert_eq!(record.external_ref.ref_id, external_ref.ref_id);
        assert_eq!(record.raw_bytes, raw.len());

        let loaded_item = store.load_item(&tool_item.id).expect("reload item");
        assert_eq!(loaded_item.artifact_refs.len(), 1);
        assert_eq!(
            load_raw_from_artifact_meta_path(&loaded_item.artifact_refs[0]).expect("blob via ref"),
            raw
        );
    }
}

