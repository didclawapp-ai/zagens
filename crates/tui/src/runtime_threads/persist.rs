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

        if let Some(ref db) = self.db {
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
            crate::thread_store_sqlite::append_event_sqlite(&db.lock().unwrap(), &record, seq)
                .map_err(|e| anyhow!("append_event sqlite: {e}"))?;
            drop(state);
            return Ok(record);
        }

        write_json_atomic(&self.state_path, &*state)?;
        drop(state);

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

        let path = self.events_path(thread_id);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("Failed to open {}", path.display()))?;
        let line = serde_json::to_string(&record)?;
        writeln!(file, "{line}").with_context(|| format!("Failed to append {}", path.display()))?;
        file.flush()
            .with_context(|| format!("Failed to flush {}", path.display()))?;
        file.sync_all()
            .with_context(|| format!("Failed to fsync {}", path.display()))?;
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

pub(super) fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let payload = serde_json::to_string_pretty(value)?;
    crate::utils::write_atomic(path, payload.as_bytes())
        .with_context(|| format!("Failed to write {}", path.display()))
}

