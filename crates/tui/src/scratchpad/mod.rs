//! Audit scratchpad store (`.deepseek/scratchpad/{run_id}/`).

pub mod cleanup;
pub mod config;
mod schema;
mod summary;

pub use schema::{
    AreaStatus, Inventory, NoteLine, is_high_severity, is_open_finding,
    is_verified_finding, parse_note_line,
};
pub use config::{ScratchpadConfig, ScratchpadConfigToml};
pub use summary::{build_layered_summary, compute_superseded_ids};

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use chrono::Utc;
use serde_json::{Value, json};

use crate::tools::spec::{ToolContext, ToolError};

static RUN_LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();

fn run_lock(run_id: &str) -> Arc<Mutex<()>> {
    let table = RUN_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = table.lock().expect("scratchpad run lock table");
    guard
        .entry(run_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// Validate `run_id` is a safe directory name (no path traversal).
pub fn validate_run_id(run_id: &str) -> Result<(), ToolError> {
    let run_id = run_id.trim();
    if run_id.is_empty() {
        return Err(ToolError::invalid_input("run_id must not be empty"));
    }
    if run_id.contains("..") || run_id.contains('/') || run_id.contains('\\') {
        return Err(ToolError::invalid_input(
            "run_id must not contain path separators or '..'",
        ));
    }
    Ok(())
}

/// Resolve scratchpad `run_id` from tool input or runtime context.
pub fn resolve_run_id(ctx: &ToolContext, explicit: Option<&str>) -> Result<String, ToolError> {
    if let Some(id) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        validate_run_id(id)?;
        return Ok(id.to_string());
    }

    if let Ok(guard) = ctx.runtime.scratchpad_run_id.lock() {
        if let Some(id) = guard.as_deref().filter(|s| !s.is_empty()) {
            validate_run_id(id)?;
            return Ok(id.to_string());
        }
    }

    if let Some(tid) = ctx.runtime.active_thread_id.as_deref() {
        validate_run_id(tid)?;
        if run_dir(ctx, tid)?.is_dir() {
            return Ok(tid.to_string());
        }
    }

    if let Some(task_id) = ctx.runtime.active_task_id.as_deref() {
        validate_run_id(task_id)?;
        if run_dir(ctx, task_id)?.is_dir() {
            return Ok(task_id.to_string());
        }
    }

    Err(ToolError::invalid_input(
        "run_id required: pass run_id explicitly or create scratchpad under thread/task id first",
    ))
}

/// Open scratchpad for a workspace when `run_id` resolves (tools / API / engine).
#[must_use]
pub fn try_open_store(
    workspace: &Path,
    run_id: Option<&str>,
    thread_id: Option<&str>,
    task_id: Option<&str>,
) -> Option<ScratchpadStore> {
    let mut ctx = ToolContext::new(workspace);
    ctx.runtime.active_thread_id = thread_id.map(str::to_string);
    ctx.runtime.active_task_id = task_id.map(str::to_string);
    let resolved = resolve_run_id(&ctx, run_id).ok()?;
    ScratchpadStore::open(&ctx, &resolved).ok()
}

fn run_dir(ctx: &ToolContext, run_id: &str) -> Result<PathBuf, ToolError> {
    validate_run_id(run_id)?;
    let rel = format!(".deepseek/scratchpad/{run_id}");
    ctx.resolve_path(&rel)
}

/// Workspace-relative display path for a run directory.
pub fn display_run_path(run_id: &str) -> String {
    format!(".deepseek/scratchpad/{run_id}")
}

/// On-disk audit scratchpad for one `run_id`.
pub struct ScratchpadStore {
    run_id: String,
    run_dir: PathBuf,
    _lock: Arc<Mutex<()>>,
}

impl ScratchpadStore {
    /// Open an existing scratchpad run directory.
    pub fn open(ctx: &ToolContext, run_id: &str) -> Result<Self, ToolError> {
        validate_run_id(run_id)?;
        let dir = run_dir(ctx, run_id)?;
        if !dir.is_dir() {
            return Err(ToolError::invalid_input(format!(
                "scratchpad run not found: {}",
                display_run_path(run_id)
            )));
        }
        Ok(Self {
            run_id: run_id.to_string(),
            run_dir: dir,
            _lock: run_lock(run_id),
        })
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    fn inventory_path(&self) -> PathBuf {
        self.run_dir.join("inventory.json")
    }

    fn notes_path(&self) -> PathBuf {
        self.run_dir.join("notes.jsonl")
    }

    pub fn read_inventory(&self) -> Result<Inventory, ToolError> {
        let path = self.inventory_path();
        let raw = fs::read_to_string(&path).map_err(|e| {
            ToolError::execution_failed(format!(
                "failed to read {}: {e}",
                path.display()
            ))
        })?;
        serde_json::from_str(&raw).map_err(|e| {
            ToolError::execution_failed(format!("invalid inventory.json: {e}"))
        })
    }

    pub fn write_inventory(&self, inventory: &Inventory) -> Result<(), ToolError> {
        let _guard = self._lock.lock().expect("scratchpad lock");
        atomic_write_json(&self.inventory_path(), inventory)
    }

    fn write_inventory_unlocked(&self, inventory: &Inventory) -> Result<(), ToolError> {
        atomic_write_json(&self.inventory_path(), inventory)
    }

    pub fn read_notes(&self) -> Result<Vec<NoteLine>, ToolError> {
        let path = self.notes_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&path).map_err(|e| {
            ToolError::execution_failed(format!("failed to read notes.jsonl: {e}"))
        })?;
        let mut notes = Vec::new();
        for (idx, line) in raw.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(trimmed).map_err(|e| {
                ToolError::execution_failed(format!(
                    "invalid notes.jsonl line {}: {e}",
                    idx + 1
                ))
            })?;
            notes.push(parse_note_line(&value, idx + 1));
        }
        Ok(notes)
    }

    pub fn count_notes_for_area(&self, area_id: &str) -> Result<usize, ToolError> {
        let notes = self.read_notes()?;
        Ok(notes
            .iter()
            .filter(|n| n.area_id == area_id)
            .count())
    }

    pub fn next_note_id(&self) -> Result<String, ToolError> {
        let notes = self.read_notes()?;
        let max_seq = notes
            .iter()
            .filter_map(|n| n.id.strip_prefix("note-"))
            .filter_map(|s| s.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        Ok(format!("note-{:03}", max_seq + 1))
    }

    pub fn append_note(&self, mut line: Value) -> Result<NoteLine, ToolError> {
        let _guard = self._lock.lock().expect("scratchpad lock");

        let inventory = self.read_inventory()?;
        let valid_ids: Vec<String> = inventory.areas.iter().map(|a| a.id.clone()).collect();

        let area_id = line
            .get("area_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let kind = line
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let is_global_meta = kind == "meta" && area_id == "_global";
        if area_id.is_empty() && !is_global_meta {
            return Err(ToolError::invalid_input(
                "area_id is required (use area_id=_global only for kind=meta)",
            ));
        }
        if !is_global_meta && area_id != "_global" && !valid_ids.iter().any(|id| id == &area_id) {
            return Err(ToolError::invalid_input(format!(
                "unknown area_id '{area_id}'; valid_area_ids: {valid_ids:?}"
            )));
        }

        let severity = line
            .get("severity")
            .and_then(|v| v.as_str())
            .map(str::to_uppercase);
        if kind == "finding" && is_high_severity(severity.as_deref()) {
            let has_file = line.get("file").and_then(|v| v.as_str()).is_some_and(|s| !s.is_empty());
            let has_line = line.get("line").and_then(|v| v.as_u64()).is_some();
            if !has_file || !has_line {
                return Err(ToolError::invalid_input(
                    "kind=finding with severity HIGH/BLOCKER requires file and line",
                ));
            }
        }

        if kind == "finding" && line.get("status").is_none() {
            if let Some(obj) = line.as_object_mut() {
                obj.insert("status".into(), json!("verified"));
            }
        }

        let note_id = self.next_note_id()?;
        let ts = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        if let Some(obj) = line.as_object_mut() {
            obj.insert("id".into(), json!(note_id));
            obj.insert("ts".into(), json!(ts));
            if !obj.contains_key("source") {
                obj.insert("source".into(), json!("main"));
            }
        }

        let note = parse_note_line(&line, 0);

        let payload = serde_json::to_string(&line).map_err(|e| {
            ToolError::execution_failed(format!("failed to serialize note line: {e}"))
        })?;

        let path = self.notes_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                ToolError::execution_failed(format!("failed to create scratchpad dir: {e}"))
            })?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| ToolError::execution_failed(format!("failed to append notes.jsonl: {e}")))?;
        writeln!(file, "{payload}").map_err(|e| {
            ToolError::execution_failed(format!("failed to write notes.jsonl: {e}"))
        })?;

        Ok(note)
    }

    pub fn list_notes(&self, area_id: &str, limit: usize) -> Result<Vec<NoteLine>, ToolError> {
        let notes = self.read_notes()?;
        let filtered: Vec<NoteLine> = notes
            .into_iter()
            .filter(|n| n.area_id == area_id)
            .collect();
        let start = filtered.len().saturating_sub(limit);
        Ok(filtered[start..].to_vec())
    }

    pub fn set_area_status(
        &self,
        area_id: &str,
        status: AreaStatus,
        area_notes: Option<&str>,
        require_min_notes: usize,
    ) -> Result<Inventory, ToolError> {
        if status == AreaStatus::Done && require_min_notes > 0 {
            let count = self.count_notes_for_area(area_id)?;
            if count < require_min_notes {
                return Err(ToolError::invalid_input(format!(
                    "area '{area_id}' has {count} note(s) but require_min_notes={require_min_notes}; \
                     call scratchpad_append first, then scratchpad_set_area(done)"
                )));
            }
        }

        let _guard = self._lock.lock().expect("scratchpad lock");
        let mut inventory = self.read_inventory()?;
        let mut found = false;
        for area in &mut inventory.areas {
            if area.id == area_id {
                area.status = status;
                if let Some(n) = area_notes {
                    area.notes = n.to_string();
                }
                found = true;
                break;
            }
        }
        if !found {
            let valid_ids: Vec<String> = inventory.areas.iter().map(|a| a.id.clone()).collect();
            return Err(ToolError::invalid_input(format!(
                "unknown area_id '{area_id}'; valid_area_ids: {valid_ids:?}"
            )));
        }
        self.write_inventory_unlocked(&inventory)?;
        Ok(inventory)
    }

    pub fn build_status(&self) -> Result<Value, ToolError> {
        let inventory = self.read_inventory()?;
        let notes = self.read_notes()?;
        let superseded = compute_superseded_ids(&notes);

        let areas_total = inventory.areas.len();
        let mut areas_done = 0usize;
        let mut areas_deferred = 0usize;
        let mut areas_in_progress = 0usize;
        let mut areas_pending = 0usize;
        let mut resume_area_id: Option<String> = None;

        for area in &inventory.areas {
            match area.status {
                AreaStatus::Done => areas_done += 1,
                AreaStatus::Deferred => areas_deferred += 1,
                AreaStatus::InProgress => {
                    areas_in_progress += 1;
                    if resume_area_id.is_none() {
                        resume_area_id = Some(area.id.clone());
                    }
                }
                AreaStatus::Pending => {
                    areas_pending += 1;
                    if resume_area_id.is_none() {
                        resume_area_id = Some(area.id.clone());
                    }
                }
            }
        }

        let mut notes_per_area: HashMap<String, usize> = HashMap::new();
        for note in &notes {
            *notes_per_area.entry(note.area_id.clone()).or_insert(0) += 1;
        }

        let findings_verified = notes
            .iter()
            .filter(|n| is_verified_finding(n, &superseded))
            .count();
        let findings_open = notes
            .iter()
            .filter(|n| is_open_finding(n, &superseded))
            .count();

        Ok(json!({
            "run_id": self.run_id,
            "path": display_run_path(&self.run_id),
            "areas_total": areas_total,
            "areas_done": areas_done,
            "areas_deferred": areas_deferred,
            "areas_in_progress": areas_in_progress,
            "areas_pending": areas_pending,
            "resume_area_id": resume_area_id,
            "notes_total": notes.len(),
            "findings_verified": findings_verified,
            "findings_open": findings_open,
            "notes_per_area": notes_per_area,
        }))
    }
}

fn atomic_write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), ToolError> {
    let payload = serde_json::to_string_pretty(value).map_err(|e| {
        ToolError::execution_failed(format!("failed to serialize JSON: {e}"))
    })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            ToolError::execution_failed(format!("failed to create directory: {e}"))
        })?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &payload).map_err(|e| {
        ToolError::execution_failed(format!("failed to write {}: {e}", tmp.display()))
    })?;
    fs::rename(&tmp, path).map_err(|e| {
        ToolError::execution_failed(format!("failed to rename {}: {e}", path.display()))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratchpad::schema::AreaStatus;
    use crate::tools::spec::ToolContext;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_workspace() -> (tempfile::TempDir, ToolContext) {
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join(format!("ws-{n}"));
        std::fs::create_dir_all(&ws).expect("mkdir ws");
        let ctx = ToolContext::new(ws);
        (dir, ctx)
    }

    fn write_fixture(ctx: &ToolContext, run_id: &str) {
        let base = ctx.workspace.join(".deepseek/scratchpad").join(run_id);
        std::fs::create_dir_all(&base).expect("mkdir");
        let inv = json!({
            "run_id": run_id,
            "areas": [
                {"id": "area-a", "path": "src/a", "status": "pending", "notes": ""}
            ]
        });
        std::fs::write(
            base.join("inventory.json"),
            serde_json::to_string_pretty(&inv).unwrap(),
        )
        .expect("write inv");
    }

    #[test]
    fn append_rejects_unknown_area() {
        let (_dir, ctx) = temp_workspace();
        write_fixture(&ctx, "test-run");
        let store = ScratchpadStore::open(&ctx, "test-run").expect("open");
        let err = store
            .append_note(json!({"area_id": "nope", "kind": "cleared", "claim": "x"}))
            .expect_err("unknown area");
        assert!(err.to_string().contains("valid_area_ids"));
    }

    #[test]
    fn set_done_requires_notes() {
        let (_dir, ctx) = temp_workspace();
        write_fixture(&ctx, "test-run-2");
        let store = ScratchpadStore::open(&ctx, "test-run-2").expect("open");
        let err = store
            .set_area_status("area-a", AreaStatus::Done, None, 1)
            .expect_err("need notes");
        assert!(err.to_string().contains("require_min_notes"));
        store
            .append_note(json!({"area_id": "area-a", "kind": "cleared", "claim": "ok"}))
            .expect("append");
        store
            .set_area_status("area-a", AreaStatus::Done, None, 1)
            .expect("done ok");
    }

    #[test]
    fn status_counts_notes() {
        let (_dir, ctx) = temp_workspace();
        write_fixture(&ctx, "test-run-3");
        let store = ScratchpadStore::open(&ctx, "test-run-3").expect("open");
        store
            .append_note(json!({"area_id": "area-a", "kind": "finding", "severity": "LOW", "file":"a.rs", "line":1, "claim":"c"}))
            .expect("append");
        let status = store.build_status().expect("status");
        assert_eq!(status["notes_total"], 1);
        assert_eq!(status["findings_verified"], 1);
    }
}
