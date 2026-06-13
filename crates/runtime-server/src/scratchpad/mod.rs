//! Audit scratchpad store (`.zagens/scratchpad/{run_id}/`).

use zagens_config::{workspace_meta_dir_read, workspace_meta_rel};

pub mod auditor;
pub mod checklist_sync;
pub mod cleanup;
pub mod config;
pub mod coverage;
pub mod import;
mod init;
pub mod inventory_template;
mod schema;
mod summary;
pub mod ui_status;

pub use import::{
    import_agent_findings, open_high_finding_ids, validate_agent_run_binding, verify_note,
};
pub use init::{default_init_areas, parse_init_areas, resolve_run_id_for_init};
pub use inventory_template::workspace_audit_inventory;

pub use auditor::{build_auditor_assignment_sections, resolve_auditor_run_id};
pub use checklist_sync::checklist_inventory_warning;
pub use config::{ScratchpadConfig, ScratchpadConfigToml};
pub use coverage::{
    CoverageGateOutcome, area_meets_deferred_quality, build_l0_status_line, compute_coverage_stats,
    coverage_gate, format_p2_defer_workflow_hint, pending_area_ids, resume_area_id_from_inventory,
};
pub use schema::{
    AreaStatus, Inventory, NoteLine, is_high_severity, is_open_finding, is_verified_finding,
    parse_note_line,
};
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

    if let Ok(guard) = ctx.runtime.wire.scratchpad_run_id.lock()
        && let Some(id) = guard.as_deref().filter(|s| !s.is_empty())
    {
        validate_run_id(id)?;
        return Ok(id.to_string());
    }

    if let Some(tid) = ctx.runtime.wire.active_thread_id.as_deref() {
        validate_run_id(tid)?;
        if run_dir(ctx, tid)?.is_dir() {
            return Ok(tid.to_string());
        }
    }

    if let Some(task_id) = ctx.runtime.wire.active_task_id.as_deref() {
        validate_run_id(task_id)?;
        if run_dir(ctx, task_id)?.is_dir() {
            return Ok(task_id.to_string());
        }
    }

    Err(ToolError::invalid_input(
        "run_id required: pass run_id explicitly or create scratchpad under thread/task id first",
    ))
}

/// Best-effort scratchpad run binding for sub-agent spawn (no error when unset).
#[must_use]
pub fn try_resolve_run_id(ctx: &ToolContext, explicit: Option<&str>) -> Option<String> {
    resolve_run_id(ctx, explicit).ok()
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
    ctx.runtime.wire.active_thread_id = thread_id.map(str::to_string);
    ctx.runtime.wire.active_task_id = task_id.map(str::to_string);
    let resolved = resolve_run_id(&ctx, run_id).ok()?;
    ScratchpadStore::open(&ctx, &resolved).ok()
}

pub(crate) fn run_dir(ctx: &ToolContext, run_id: &str) -> Result<PathBuf, ToolError> {
    validate_run_id(run_id)?;
    let rel = workspace_meta_rel(&format!("scratchpad/{run_id}"));
    ctx.resolve_path(&rel)
}

/// Workspace-relative display path for a run directory.
pub fn display_run_path(run_id: &str) -> String {
    workspace_meta_rel(&format!("scratchpad/{run_id}"))
}

/// Pick the newest scratchpad run under a workspace (by `inventory.json` mtime).
///
/// Used by tests and internal tooling only — **not** for thread status API. Audit scratchpad
/// UI/status is bound to `thread.scratchpad_run_id` so other sessions do not inherit a prior run.
#[must_use]
pub fn discover_scratchpad_run_id_for_ui(workspace: &Path) -> Option<String> {
    let base = workspace_meta_dir_read(workspace).join("scratchpad");
    if !base.is_dir() {
        return None;
    }
    let mut candidates: Vec<(std::time::SystemTime, String)> = Vec::new();
    let entries = fs::read_dir(&base).ok()?;
    for entry in entries.flatten() {
        if !entry.file_type().ok()?.is_dir() {
            continue;
        }
        let run_id = entry.file_name().to_string_lossy().into_owned();
        if validate_run_id(&run_id).is_err() {
            continue;
        }
        let inv = entry.path().join("inventory.json");
        if !inv.is_file() {
            continue;
        }
        let mtime = inv.metadata().ok()?.modified().ok()?;
        candidates.push((mtime, run_id));
    }
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by_key(|a| std::cmp::Reverse(a.0));
    Some(candidates[0].1.clone())
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
            ToolError::execution_failed(format!("failed to read {}: {e}", path.display()))
        })?;
        serde_json::from_str(&raw)
            .map_err(|e| ToolError::execution_failed(format!("invalid inventory.json: {e}")))
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
        let raw = fs::read_to_string(&path)
            .map_err(|e| ToolError::execution_failed(format!("failed to read notes.jsonl: {e}")))?;
        let mut notes = Vec::new();
        for (idx, line) in raw.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(trimmed).map_err(|e| {
                ToolError::execution_failed(format!("invalid notes.jsonl line {}: {e}", idx + 1))
            })?;
            notes.push(parse_note_line(&value, idx + 1));
        }
        Ok(notes)
    }

    pub fn count_notes_for_area(&self, area_id: &str) -> Result<usize, ToolError> {
        let notes = self.read_notes()?;
        Ok(notes.iter().filter(|n| n.area_id == area_id).count())
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
            let has_file = line
                .get("file")
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.is_empty());
            let has_line = line.get("line").and_then(|v| v.as_u64()).is_some();
            if !has_file || !has_line {
                return Err(ToolError::invalid_input(
                    "kind=finding with severity HIGH/BLOCKER requires file and line",
                ));
            }
        }

        if kind == "finding"
            && line.get("status").is_none()
            && let Some(obj) = line.as_object_mut()
        {
            obj.insert("status".into(), json!("open"));
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
            .map_err(|e| {
                ToolError::execution_failed(format!("failed to append notes.jsonl: {e}"))
            })?;
        writeln!(file, "{payload}").map_err(|e| {
            ToolError::execution_failed(format!("failed to write notes.jsonl: {e}"))
        })?;

        Ok(note)
    }

    pub fn list_notes(&self, area_id: &str, limit: usize) -> Result<Vec<NoteLine>, ToolError> {
        let notes = self.read_notes()?;
        let filtered: Vec<NoteLine> = notes.into_iter().filter(|n| n.area_id == area_id).collect();
        let start = filtered.len().saturating_sub(limit);
        Ok(filtered[start..].to_vec())
    }

    pub fn set_area_status(
        &self,
        area_id: &str,
        status: AreaStatus,
        area_notes: Option<&str>,
        require_min_notes: usize,
        config: &ScratchpadConfig,
    ) -> Result<Inventory, ToolError> {
        if status == AreaStatus::Done {
            let open_high = open_high_finding_ids(self, area_id)?;
            if !open_high.is_empty() {
                return Err(ToolError::invalid_input(format!(
                    "area '{area_id}' has open HIGH/BLOCKER findings {open_high:?}; \
                     call scratchpad_verify_note for each after read_file/grep_files, then scratchpad_set_area(done)"
                )));
            }
        }

        if matches!(status, AreaStatus::Done | AreaStatus::Deferred) && require_min_notes > 0 {
            let mut count = self.count_notes_for_area(area_id)?;
            if count < require_min_notes
                && let Some(text) = area_notes
                && !text.trim().is_empty()
            {
                // Models often pass a summary in `notes` without scratchpad_append;
                // coalesce into notes.jsonl so batch set_area(done) does not fail the turn.
                self.append_note(json!({
                    "area_id": area_id,
                    "kind": "meta",
                    "claim": text.trim(),
                }))?;
                count = self.count_notes_for_area(area_id)?;
            }
            if count < require_min_notes {
                return Err(ToolError::invalid_input(format!(
                    "area '{area_id}' has {count} note(s) but require_min_notes={require_min_notes}; \
                     call scratchpad_append first, then scratchpad_set_area({})",
                    status.as_str()
                )));
            }
        }

        if status == AreaStatus::Deferred && config.require_deferred_meta {
            let notes = self.read_notes()?;
            if !area_meets_deferred_quality(area_id, &notes) {
                let inventory = self.read_inventory()?;
                let pending = pending_area_ids(&inventory);
                let workflow = format_p2_defer_workflow_hint(&pending, 6);
                return Err(ToolError::invalid_input(format!(
                    "area '{area_id}': deferred requires scratchpad_append(kind=meta, area_id=\"{area_id}\", \
                     claim=<non-empty defer reason>) before scratchpad_set_area(deferred). {workflow}"
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

        let mut findings_open_high = 0usize;
        let mut findings_open_medium = 0usize;
        let mut findings_open_low = 0usize;
        let mut findings_verified_high = 0usize;
        for note in &notes {
            if !note.kind.eq_ignore_ascii_case("finding") || superseded.contains(&note.id) {
                continue;
            }
            let high = is_high_severity(note.severity.as_deref());
            let verified = is_verified_finding(note, &superseded);
            let open = is_open_finding(note, &superseded);
            if verified && high {
                findings_verified_high += 1;
            } else if open {
                if high {
                    findings_open_high += 1;
                } else if note
                    .severity
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case("MEDIUM"))
                {
                    findings_open_medium += 1;
                } else {
                    findings_open_low += 1;
                }
            }
        }

        let areas: Vec<Value> = inventory
            .areas
            .iter()
            .map(|area| {
                let notes_count = notes_per_area.get(&area.id).copied().unwrap_or(0);
                json!({
                    "id": area.id,
                    "path": area.path,
                    "status": area.status.as_str(),
                    "notes_count": notes_count,
                })
            })
            .collect();

        let coverage_cfg = ScratchpadConfig::default();
        let coverage = compute_coverage_stats(&inventory, &notes, &coverage_cfg);
        let quality_gaps = coverage::areas_failing_quality_gate(&inventory, &notes, &coverage_cfg);
        let quality_gap_ids: Vec<&str> = quality_gaps.iter().map(|g| g.id.as_str()).collect();

        Ok(json!({
            "run_id": self.run_id,
            "path": display_run_path(&self.run_id),
            "areas_total": areas_total,
            "areas_done": areas_done,
            "areas_deferred": areas_deferred,
            "areas_in_progress": areas_in_progress,
            "areas_pending": areas_pending,
            "areas_accounted": coverage.areas_accounted,
            "accounted_ratio": coverage.accounted_ratio,
            "areas_failing_quality_gate": quality_gap_ids,
            "resume_area_id": resume_area_id,
            "notes_total": notes.len(),
            "findings_verified": findings_verified,
            "findings_open": findings_open,
            "findings_verified_high": findings_verified_high,
            "findings_open_high": findings_open_high,
            "findings_open_medium": findings_open_medium,
            "findings_open_low": findings_open_low,
            "notes_per_area": notes_per_area,
            "areas": areas,
            "contract_hints": build_contract_hints(
                areas_pending,
                areas_done,
                areas_deferred,
                notes.len(),
                &coverage,
                &coverage_cfg,
            ),
        }))
    }
}

fn build_contract_hints(
    areas_pending: usize,
    areas_done: usize,
    areas_deferred: usize,
    notes_total: usize,
    coverage: &coverage::CoverageStats,
    config: &ScratchpadConfig,
) -> Vec<&'static str> {
    let mut hints = Vec::new();
    if areas_pending > 0 && notes_total > 0 {
        hints.push(
            "P2 blocked: every pending area needs scratchpad_set_area(done|deferred) before write_file audit report",
        );
        hints.push(
            "defer rule: ONE area per step — scratchpad_append(meta) then scratchpad_set_area(deferred); never batch multiple deferred calls",
        );
    }
    if areas_pending > 0 && notes_total > 0 && areas_done + areas_deferred == 0 {
        hints.push(
            "checklist completed rows must match inventory — use scratchpad_set_area, not checklist alone",
        );
    }
    if areas_pending == 0 && areas_done + areas_deferred > 0 && notes_total > 0 {
        if coverage.accounted_ratio < config.coverage_hard_ratio
            && config.coverage_hard_block_enabled
        {
            hints.push(
                "P2 blocked: accounted_ratio below 60% — done areas need kind=finding or kind=cleared (meta-only does not count); see areas_failing_quality_gate",
            );
        } else if config.coverage_reviewed_hard_block_enabled
            && coverage.reviewed_ratio < config.coverage_reviewed_hard_ratio
        {
            hints.push(
                "P2 blocked: reviewed_ratio below 40% — deferred areas do not count; continue P1 or append _global meta partial_closeout if user approved partial report",
            );
        } else {
            hints
                .push("inventory closed — synthesize report from verified findings via write_file");
        }
    }
    hints
}

pub(crate) fn atomic_write_json(
    path: &Path,
    value: &impl serde::Serialize,
) -> Result<(), ToolError> {
    let payload = serde_json::to_string_pretty(value)
        .map_err(|e| ToolError::execution_failed(format!("failed to serialize JSON: {e}")))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| ToolError::execution_failed(format!("failed to create directory: {e}")))?;
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
    use crate::scratchpad::config::ScratchpadConfig;
    use crate::scratchpad::schema::AreaStatus;
    use crate::tools::spec::ToolContext;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use zagens_config::workspace_meta_dir;

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
        let base = workspace_meta_dir(&ctx.workspace)
            .join("scratchpad")
            .join(run_id);
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
        let cfg = ScratchpadConfig::default();
        let err = store
            .set_area_status("area-a", AreaStatus::Done, None, 1, &cfg)
            .expect_err("need notes");
        assert!(err.to_string().contains("require_min_notes"));
        store
            .append_note(json!({"area_id": "area-a", "kind": "cleared", "claim": "ok"}))
            .expect("append");
        store
            .set_area_status("area-a", AreaStatus::Done, None, 1, &cfg)
            .expect("done ok");
    }

    #[test]
    fn set_done_coalesces_notes_param_without_prior_append() {
        let (_dir, ctx) = temp_workspace();
        write_fixture(&ctx, "test-run-coalesce");
        let store = ScratchpadStore::open(&ctx, "test-run-coalesce").expect("open");
        let cfg = ScratchpadConfig::default();
        store
            .set_area_status(
                "area-a",
                AreaStatus::Done,
                Some("Sub-agent summary: absdiff size check missing (HIGH)"),
                1,
                &cfg,
            )
            .expect("done with coalesced note");
        assert_eq!(store.count_notes_for_area("area-a").expect("count"), 1);
    }

    #[test]
    fn set_deferred_requires_meta() {
        let (_dir, ctx) = temp_workspace();
        write_fixture(&ctx, "test-run-deferred");
        let store = ScratchpadStore::open(&ctx, "test-run-deferred").expect("open");
        let cfg = ScratchpadConfig::default();
        store
            .append_note(json!({"area_id": "area-a", "kind": "cleared", "claim": "placeholder"}))
            .expect("append");
        let err = store
            .set_area_status("area-a", AreaStatus::Deferred, None, 1, &cfg)
            .expect_err("need meta");
        assert!(err.to_string().contains("kind=meta"));
        store
            .append_note(json!({
                "area_id": "area-a",
                "kind": "meta",
                "claim": "skipped: out of scope"
            }))
            .expect("append meta");
        store
            .set_area_status("area-a", AreaStatus::Deferred, None, 1, &cfg)
            .expect("deferred ok");
    }

    #[test]
    fn discover_run_id_picks_newest_inventory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path();
        let base = workspace_meta_dir(ws).join("scratchpad");
        fs::create_dir_all(base.join("older-run")).expect("mkdir");
        fs::write(
            base.join("older-run/inventory.json"),
            r#"{"run_id":"older-run","areas":[]}"#,
        )
        .expect("write");
        std::thread::sleep(std::time::Duration::from_millis(60));
        fs::create_dir_all(base.join("newer-run")).expect("mkdir");
        fs::write(
            base.join("newer-run/inventory.json"),
            r#"{"run_id":"newer-run","areas":[]}"#,
        )
        .expect("write");
        let picked = discover_scratchpad_run_id_for_ui(ws).expect("discover");
        assert_eq!(picked, "newer-run");
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
        assert_eq!(status["findings_open"], 1);
        assert_eq!(status["findings_verified"], 0);
        let areas = status["areas"].as_array().expect("areas array");
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0]["id"], "area-a");
        assert_eq!(areas[0]["notes_count"], 1);
        assert_eq!(areas[0]["status"], "pending");
    }
}
