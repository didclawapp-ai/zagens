//! Zagens UI enrichments for `GET /v1/threads/{id}/scratchpad/status` (Phase D2).

use std::path::Path;

use serde_json::{Value, json};

use deepseek_runtime_orchestrator::runtime_threads::types::ThreadRecord;

/// Count checklist items from a `checklist_write` snapshot JSON.
#[must_use]
pub fn count_checklist_items(snapshot: &Value) -> (usize, usize) {
    let Some(items) = snapshot.get("items").and_then(|v| v.as_array()) else {
        return (0, 0);
    };
    let total = items.len();
    let completed = items
        .iter()
        .filter(|item| {
            item.get("status")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("completed"))
        })
        .count();
    (completed, total)
}

/// Mechanical contract warnings for the audit dashboard (§6.13 D2).
#[must_use]
pub fn compute_contract_warnings(status: &Value, checklist_completed: usize) -> Vec<String> {
    let mut warnings = Vec::new();

    let notes_total = status.get("notes_total").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let areas_done = status.get("areas_done").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let areas_deferred = status
        .get("areas_deferred")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let areas_in_progress = status
        .get("areas_in_progress")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let accounted = areas_done + areas_deferred + areas_in_progress;

    if notes_total > 0 && accounted == 0 {
        warnings.push("notes_without_accounted".to_string());
    }

    if checklist_completed > 0 && accounted == 0 {
        warnings.push("checklist_inventory_mismatch".to_string());
    } else if checklist_completed > 0 && areas_done == 0 && areas_in_progress == 0 {
        warnings.push("checklist_inventory_mismatch".to_string());
    }

    warnings
}

/// Merge thread-local fields into a `build_status()` JSON object.
pub fn enrich_status_for_thread_ui(status: &mut Value, checklist_json: Option<&str>) {
    let (checklist_completed, checklist_total) = checklist_json
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .map(|v| count_checklist_items(&v))
        .unwrap_or((0, 0));

    let warnings = compute_contract_warnings(status, checklist_completed);

    if let Some(obj) = status.as_object_mut() {
        obj.insert(
            "checklist_completed".to_string(),
            json!(checklist_completed),
        );
        obj.insert("checklist_total".to_string(), json!(checklist_total));
        obj.insert("contract_warnings".to_string(), json!(warnings));
    }
}

fn build_run_status(
    workspace: &Path,
    run_id: &str,
    thread_id: &str,
    task_id: Option<&str>,
) -> Option<Value> {
    let store = super::try_open_store(workspace, Some(run_id), Some(thread_id), task_id)?;
    store.build_status().ok()
}

/// Build panel JSON: latest run at top level + `previous_runs` (newest-first, folded in UI).
pub fn build_thread_scratchpad_panel_status(
    thread: &ThreadRecord,
    checklist_json: Option<&str>,
) -> Option<Value> {
    let history = thread.scratchpad_history();
    let latest_id = history.last()?.clone();
    let mut latest = build_run_status(
        &thread.workspace,
        &latest_id,
        &thread.id,
        thread.task_id.as_deref(),
    )?;
    enrich_status_for_thread_ui(&mut latest, checklist_json);

    let mut previous_runs: Vec<Value> = Vec::new();
    for run_id in history.iter().rev().skip(1) {
        if let Some(st) = build_run_status(
            &thread.workspace,
            run_id,
            &thread.id,
            thread.task_id.as_deref(),
        ) {
            previous_runs.push(st);
        }
    }

    if let Some(obj) = latest.as_object_mut() {
        if !previous_runs.is_empty() {
            obj.insert("previous_runs".to_string(), json!(previous_runs));
        }
    }
    Some(latest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use std::path::PathBuf;

    use deepseek_core::coherence::CoherenceState;
    use deepseek_runtime_orchestrator::runtime_threads::types::ThreadRecord;

    #[test]
    fn thread_scratchpad_history_backfills_from_active_run() {
        let thread = ThreadRecord {
            schema_version: 2,
            id: "thr_a".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            model: "m".to_string(),
            workspace: PathBuf::from("/tmp"),
            mode: "agent".to_string(),
            allow_shell: false,
            trust_mode: false,
            auto_approve: false,
            latest_turn_id: None,
            latest_response_bookmark: None,
            archived: false,
            system_prompt: None,
            task_id: None,
            title: None,
            task_type: "code".to_string(),
            coherence_state: CoherenceState::default(),
            scratchpad_run_id: Some("run-1".to_string()),
            scratchpad_run_history: None,
            checklist_snapshot: None,
            plan_snapshot: None,
        };
        assert_eq!(thread.scratchpad_history(), vec!["run-1".to_string()]);
    }

    #[test]
    fn record_scratchpad_run_appends_and_promotes_latest() {
        let mut thread = ThreadRecord {
            schema_version: 2,
            id: "thr_b".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            model: "m".to_string(),
            workspace: PathBuf::from("/tmp"),
            mode: "agent".to_string(),
            allow_shell: false,
            trust_mode: false,
            auto_approve: false,
            latest_turn_id: None,
            latest_response_bookmark: None,
            archived: false,
            system_prompt: None,
            task_id: None,
            title: None,
            task_type: "code".to_string(),
            coherence_state: CoherenceState::default(),
            scratchpad_run_id: None,
            scratchpad_run_history: None,
            checklist_snapshot: None,
            plan_snapshot: None,
        };
        thread.record_scratchpad_run("audit-1");
        thread.record_scratchpad_run("audit-2");
        assert_eq!(
            thread.scratchpad_history(),
            vec!["audit-1".to_string(), "audit-2".to_string()]
        );
        assert_eq!(thread.scratchpad_run_id.as_deref(), Some("audit-2"));
        thread.record_scratchpad_run("audit-1");
        assert_eq!(
            thread.scratchpad_history(),
            vec!["audit-2".to_string(), "audit-1".to_string()]
        );
    }

    #[test]
    fn checklist_counts_completed() {
        let snap = json!({
            "items": [
                {"id": 1, "status": "completed"},
                {"id": 2, "status": "pending"},
                {"id": 3, "status": "completed"}
            ]
        });
        assert_eq!(count_checklist_items(&snap), (2, 3));
    }

    #[test]
    fn warns_checklist_inventory_mismatch() {
        let status = json!({
            "notes_total": 0,
            "areas_done": 0,
            "areas_deferred": 0,
            "areas_in_progress": 0
        });
        let w = compute_contract_warnings(&status, 5);
        assert!(w.contains(&"checklist_inventory_mismatch".to_string()));
    }
}
