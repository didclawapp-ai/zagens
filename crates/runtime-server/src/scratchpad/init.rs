//! Scratchpad bootstrap (`scratchpad_init` + HTTP init).

use std::fs;

use chrono::Utc;
use serde_json::Value;

use crate::tools::spec::{ToolContext, ToolError};

use super::schema::{AreaStatus, Inventory, InventoryArea};
use super::{ScratchpadStore, display_run_path, run_dir, validate_run_id};

/// Default inventory when `scratchpad_init` omits `areas`.
#[must_use]
pub fn default_init_areas() -> Vec<InventoryArea> {
    vec![InventoryArea {
        id: "workspace".to_string(),
        path: ".".to_string(),
        status: AreaStatus::Pending,
        notes: String::new(),
    }]
}

fn validate_area_id(area_id: &str) -> Result<(), ToolError> {
    let area_id = area_id.trim();
    if area_id.is_empty() {
        return Err(ToolError::invalid_input("area id must not be empty"));
    }
    if area_id.contains("..") || area_id.contains('/') || area_id.contains('\\') {
        return Err(ToolError::invalid_input(
            "area id must not contain path separators or '..'",
        ));
    }
    Ok(())
}

/// Parse tool/API `areas` payload into inventory rows (all start as `pending`).
pub fn parse_init_areas(raw: &[Value]) -> Result<Vec<InventoryArea>, ToolError> {
    if raw.is_empty() {
        return Err(ToolError::invalid_input("areas must not be empty"));
    }
    let mut areas = Vec::with_capacity(raw.len());
    let mut seen = std::collections::HashSet::new();
    for item in raw {
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::invalid_input("each area requires a non-empty id"))?;
        validate_area_id(id)?;
        if !seen.insert(id.to_string()) {
            return Err(ToolError::invalid_input(format!(
                "duplicate area id '{id}'"
            )));
        }
        let path = item
            .get("path")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::invalid_input(format!("area '{id}' requires path")))?;
        let notes = item
            .get("notes")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        areas.push(InventoryArea {
            id: id.to_string(),
            path: path.to_string(),
            status: AreaStatus::Pending,
            notes,
        });
    }
    Ok(areas)
}

/// Resolve `run_id` for init — defaults to active thread/task id (directory may not exist yet).
pub fn resolve_run_id_for_init(
    ctx: &ToolContext,
    explicit: Option<&str>,
) -> Result<String, ToolError> {
    if let Some(id) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        validate_run_id(id)?;
        return Ok(id.to_string());
    }
    if let Some(tid) = ctx
        .runtime
        .wire
        .active_thread_id
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        validate_run_id(tid)?;
        return Ok(tid.to_string());
    }
    if let Some(task_id) = ctx
        .runtime
        .wire
        .active_task_id
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        validate_run_id(task_id)?;
        return Ok(task_id.to_string());
    }
    Err(ToolError::invalid_input(
        "run_id required: pass run_id or bind an active thread_id/task_id",
    ))
}

impl ScratchpadStore {
    /// Create `.zagens/scratchpad/{run_id}/` with `inventory.json` + empty `notes.jsonl`.
    /// Idempotent when the run directory already has a valid inventory.
    pub fn init(
        ctx: &ToolContext,
        run_id: &str,
        areas: Vec<InventoryArea>,
        scope: Option<&str>,
    ) -> Result<Self, ToolError> {
        validate_run_id(run_id)?;
        if areas.is_empty() {
            return Err(ToolError::invalid_input("areas must not be empty"));
        }
        let dir = run_dir(ctx, run_id)?;
        let inventory_path = dir.join("inventory.json");
        if dir.is_dir() && inventory_path.is_file() {
            return Self::open(ctx, run_id);
        }
        fs::create_dir_all(&dir).map_err(|e| {
            ToolError::execution_failed(format!(
                "failed to create scratchpad dir {}: {e}",
                display_run_path(run_id)
            ))
        })?;
        let created_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let inventory = Inventory {
            run_id: run_id.to_string(),
            created_at,
            completed_at: None,
            scope: scope.map(str::to_string),
            areas,
        };
        super::atomic_write_json(&inventory_path, &inventory)?;
        let notes_path = dir.join("notes.jsonl");
        if !notes_path.exists() {
            fs::write(&notes_path, "").map_err(|e| {
                ToolError::execution_failed(format!("failed to create notes.jsonl: {e}"))
            })?;
        }
        Self::open(ctx, run_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::spec::ToolContext;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_ctx() -> (tempfile::TempDir, ToolContext) {
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join(format!("ws-{n}"));
        std::fs::create_dir_all(&ws).expect("mkdir ws");
        (dir, ToolContext::new(ws))
    }

    #[test]
    fn init_creates_inventory_and_notes() {
        let (_dir, mut ctx) = temp_ctx();
        ctx.runtime.wire.active_thread_id = Some("thr_test".to_string());
        let run_id = resolve_run_id_for_init(&ctx, None).expect("run id");
        assert_eq!(run_id, "thr_test");
        let store = ScratchpadStore::init(&ctx, &run_id, default_init_areas(), None).expect("init");
        let inv = store.read_inventory().expect("inventory");
        assert_eq!(inv.areas.len(), 1);
        assert_eq!(inv.areas[0].id, "workspace");
        assert!(store.run_dir().join("notes.jsonl").is_file());
    }

    #[test]
    fn init_is_idempotent() {
        let (_dir, mut ctx) = temp_ctx();
        ctx.runtime.wire.active_thread_id = Some("thr_idem".to_string());
        let areas = default_init_areas();
        ScratchpadStore::init(&ctx, "thr_idem", areas.clone(), None).expect("first");
        let store = ScratchpadStore::init(&ctx, "thr_idem", areas, None).expect("second");
        assert_eq!(
            store.read_inventory().expect("inv").areas[0].id,
            "workspace"
        );
    }

    #[test]
    fn parse_init_areas_rejects_duplicates() {
        let raw = vec![
            json!({"id": "a", "path": "src"}),
            json!({"id": "a", "path": "docs"}),
        ];
        let err = parse_init_areas(&raw).expect_err("dup");
        assert!(err.to_string().contains("duplicate"));
    }
}
