//! Checklist ↔ scratchpad inventory alignment warnings (audit-repo dual-track).

use crate::tools::spec::ToolContext;

use super::schema::AreaStatus;
use super::{ScratchpadStore, resolve_run_id};

/// Warn when sidebar checklist progress diverges from scratchpad inventory.
#[must_use]
pub fn checklist_inventory_warning(
    ctx: &ToolContext,
    checklist_completed: usize,
) -> Option<String> {
    let run_id = ctx
        .audit_scratchpad_run_id
        .clone()
        .or_else(|| resolve_run_id(ctx, None).ok())?;
    let store = ScratchpadStore::open(ctx, &run_id).ok()?;
    let inventory = store.read_inventory().ok()?;
    if inventory.areas.is_empty() {
        return None;
    }

    let areas_done = inventory
        .areas
        .iter()
        .filter(|a| a.status == AreaStatus::Done)
        .count();
    let areas_deferred = inventory
        .areas
        .iter()
        .filter(|a| a.status == AreaStatus::Deferred)
        .count();
    let areas_in_progress = inventory
        .areas
        .iter()
        .filter(|a| a.status == AreaStatus::InProgress)
        .count();
    let inventory_accounted = areas_done + areas_deferred + areas_in_progress;

    if checklist_completed > 0 && inventory_accounted == 0 {
        return Some(format!(
            "WARNING checklist_inventory_mismatch: checklist shows {checklist_completed} completed \
             but scratchpad inventory has 0 done/deferred/in_progress areas. \
             Call scratchpad_set_area(done|deferred) for each completed checklist row before \
             marking more items completed."
        ));
    }

    if checklist_completed > areas_done + areas_deferred {
        return Some(format!(
            "WARNING checklist_inventory_mismatch: checklist completed ({checklist_completed}) \
             exceeds inventory closed ({areas_done} done + {areas_deferred} deferred). \
             Sync scratchpad_set_area before advancing checklist."
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratchpad::{ScratchpadStore, default_init_areas};
    use crate::tools::spec::ToolContext;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_ctx_with_scratchpad() -> (tempfile::TempDir, ToolContext, String) {
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join(format!("ws-{n}"));
        std::fs::create_dir_all(&ws).expect("mkdir ws");
        let mut ctx = ToolContext::new(ws.clone());
        let run_id = format!("thr-{n}");
        ctx.runtime.wire.active_thread_id = Some(run_id.clone());
        ctx.audit_scratchpad_run_id = Some(run_id.clone());
        ScratchpadStore::init(&ctx, &run_id, default_init_areas(), None).expect("init");
        (dir, ctx, run_id)
    }

    #[test]
    fn warns_when_checklist_completed_but_inventory_untouched() {
        let (_dir, ctx, _run_id) = temp_ctx_with_scratchpad();
        let warn = checklist_inventory_warning(&ctx, 5).expect("warn");
        assert!(warn.contains("checklist_inventory_mismatch"));
        assert!(warn.contains("scratchpad_set_area"));
    }

    #[test]
    fn silent_when_no_active_scratchpad() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = ToolContext::new(dir.path().join("ws"));
        assert!(checklist_inventory_warning(&ctx, 3).is_none());
    }
}
