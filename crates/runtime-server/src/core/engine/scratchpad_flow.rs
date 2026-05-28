//! Audit scratchpad engine hooks (B3/B4/B3b, Phase C0/C1).

use std::path::Path;

use crate::models::{ContentBlock, Message, SystemBlock, SystemPrompt};
use crate::scratchpad::config::ScratchpadConfig;
use crate::scratchpad::{
    AreaStatus, CoverageGateOutcome, ScratchpadStore, build_layered_summary,
    build_l0_status_line, compute_coverage_stats, coverage_gate, display_run_path,
    resume_area_id_from_inventory,
};

// M5: `ScratchpadStepState` moved to
// `deepseek_core::engine::scratchpad_state` (spike §3 row #28 + R12 —
// the small state struct migrated; the heavy flow helpers in this
// 484-LOC file stay tui-side). The re-export shim below keeps every
// `use crate::core::engine::scratchpad_flow::ScratchpadStepState`
// caller compiling unchanged (engine state, host_impl turn-loop
// bookkeeping, message_handlers reset, tests).
pub use deepseek_core::engine::ScratchpadStepState;
pub use deepseek_runtime_adapters::scratchpad_gates::{
    check_task_create_audit_gate, check_write_file_audit_report_gate, is_audit_deliverable_path,
};

const READONLY_TOOLS: &[&str] = &[
    "read_file",
    "list_dir",
    "grep_files",
    "glob_files",
    "file_info",
    "file_search",
    "git_status",
    "git_diff",
    "git_log",
    "git_show",
    "git_blame",
    "diagnostics",
    "scratchpad_status",
    "scratchpad_list_notes",
    "project_map",
    "recall_archive",
];

pub fn is_readonly_tool(name: &str) -> bool {
    READONLY_TOOLS.contains(&name)
}

pub fn is_scratchpad_write_tool(name: &str) -> bool {
    matches!(
        name,
        "scratchpad_init"
            | "scratchpad_append"
            | "scratchpad_set_area"
            | "scratchpad_verify_note"
            | "scratchpad_import_agent"
    )
}

pub fn record_tool_outcome(state: &mut ScratchpadStepState, tool_name: &str, success: bool) {
    if !success {
        return;
    }
    if is_scratchpad_write_tool(tool_name) {
        state.scratchpad_writes_this_step += 1;
    } else if is_readonly_tool(tool_name) {
        state.readonly_tool_successes += 1;
    }
}

pub fn open_store(
    workspace: &Path,
    run_id: Option<&str>,
    thread_id: Option<&str>,
    task_id: Option<&str>,
) -> Option<ScratchpadStore> {
    crate::scratchpad::try_open_store(workspace, run_id, thread_id, task_id)
}

/// C0 — pin scratchpad paths for compaction (§6.12.3).
pub fn extend_compaction_paths(
    workspace: &Path,
    run_id: Option<&str>,
    paths: &mut Vec<String>,
) {
    let Some(store) = open_store(workspace, run_id, None, None) else {
        return;
    };
    let root = display_run_path(store.run_id());
    if !paths.iter().any(|p| p == &root) {
        paths.push(root.clone());
    }
    let inv = format!("{root}/inventory.json");
    if !paths.iter().any(|p| p == &inv) {
        paths.push(inv);
    }
}

/// C0 — L0-only system block appended after compaction (§6.12.3).
#[must_use]
pub fn scratchpad_compaction_system_prompt(
    workspace: &Path,
    run_id: Option<&str>,
    config: &ScratchpadConfig,
) -> Option<SystemPrompt> {
    if !config.enabled {
        return None;
    }
    let store = open_store(workspace, run_id, None, None)?;
    let inventory = store.read_inventory().ok()?;
    let notes = store.read_notes().ok()?;
    let stats = compute_coverage_stats(&inventory, &notes, config);
    let resume = resume_area_id_from_inventory(&inventory);
    let line = build_l0_status_line(store.run_id(), &stats, &resume);
    Some(SystemPrompt::Blocks(vec![SystemBlock {
        block_type: "text".to_string(),
        text: format!("[scratchpad L0] {line}"),
        cache_control: None,
    }]))
}

/// B3b — line for cycle handoff / structured state when a scratchpad exists.
#[must_use]
pub fn scratchpad_handoff_line(workspace: &Path, run_id: Option<&str>) -> Option<String> {
    let store = open_store(workspace, run_id, None, None)?;
    let config = ScratchpadConfig::default();
    let inventory = store.read_inventory().ok()?;
    let notes = store.read_notes().ok()?;
    let stats = compute_coverage_stats(&inventory, &notes, &config);
    let resume = resume_area_id_from_inventory(&inventory);
    Some(format!(
        "Active audit scratchpad: {} ({})",
        display_run_path(store.run_id()),
        build_l0_status_line(store.run_id(), &stats, &resume),
    ))
}

fn current_focus_area(store: &ScratchpadStore) -> Option<(String, String)> {
    let inventory = store.read_inventory().ok()?;
    for area in &inventory.areas {
        if area.status == AreaStatus::InProgress {
            return Some((area.id.clone(), area.path.clone()));
        }
    }
    for area in &inventory.areas {
        if area.status == AreaStatus::Pending {
            return Some((area.id.clone(), area.path.clone()));
        }
    }
    None
}

fn inventory_complete(store: &ScratchpadStore) -> bool {
    let Ok(inventory) = store.read_inventory() else {
        return false;
    };
    !inventory.areas.is_empty()
        && inventory.areas.iter().all(|a| {
            matches!(
                a.status,
                AreaStatus::Done | AreaStatus::Deferred
            )
        })
}

pub fn user_prompt_triggers_report_summary(prompt: &str, config: &ScratchpadConfig) -> bool {
    let lower = prompt.to_lowercase();
    config
        .inject_on_report_keywords
        .iter()
        .any(|kw| lower.contains(&kw.to_lowercase()))
}

fn wrap_scratchpad_summary(run_id: &str, body: &str) -> Message {
    Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text: format!("<scratchpad_summary run_id=\"{run_id}\">\n{body}\n</scratchpad_summary>"),
            cache_control: None,
        }],
    }
}

/// B3 + C1 — inject layered summary or coverage warn/block as a synthetic user message.
pub fn build_report_summary_message(
    workspace: &Path,
    run_id: Option<&str>,
    config: &ScratchpadConfig,
) -> Option<Message> {
    if !config.enabled {
        return None;
    }
    let store = open_store(workspace, run_id, None, None)?;
    let inventory = store.read_inventory().ok()?;
    let notes = store.read_notes().ok()?;
    let run_id = store.run_id().to_string();

    match coverage_gate(&inventory, &notes, config) {
        CoverageGateOutcome::Block { reason, stats } => {
            let l0 = build_l0_status_line(
                &run_id,
                &stats,
                &resume_area_id_from_inventory(&inventory),
            );
            let body = format!("[L0] {l0};\nBLOCKED: {reason}");
            Some(wrap_scratchpad_summary(&run_id, &body))
        }
        CoverageGateOutcome::Warn {
            warning_text,
            stats,
        } => {
            let l0 = build_l0_status_line(
                &run_id,
                &stats,
                &resume_area_id_from_inventory(&inventory),
            );
            let body = format!("[L0] {l0};\n{warning_text}");
            Some(wrap_scratchpad_summary(&run_id, &body))
        }
        CoverageGateOutcome::Allow { .. } => {
            let summary =
                build_layered_summary(&inventory, &notes, config.inject_summary_max_chars, config);
            Some(wrap_scratchpad_summary(&run_id, &summary))
        }
    }
}

/// B4 — reminder after many readonly tools without scratchpad writes.
pub fn build_readonly_reminder_message(
    workspace: &Path,
    run_id: Option<&str>,
    config: &ScratchpadConfig,
    step: &ScratchpadStepState,
) -> Option<Message> {
    if !config.enabled || !config.remind_enabled {
        return None;
    }
    if step.scratchpad_writes_this_step > 0 {
        return None;
    }
    if step.readonly_tool_successes < config.remind_after_readonly_tools {
        return None;
    }
    let store = open_store(workspace, run_id, None, None)?;
    let (area_id, area_path) = current_focus_area(&store)?;
    let text = format!(
        "当前审查区 **`{area_id}`**（`{area_path}`）已连续 {}+ 次只读工具调用但未更新 scratchpad。\
         请先 `scratchpad_append`（≥1 条，含 area_id），再 `scratchpad_set_area`。\
         路径：{}",
        step.readonly_tool_successes,
        display_run_path(store.run_id())
    );
    Some(Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text,
            cache_control: None,
        }],
    })
}

/// Inject summary before a final answer when inventory is complete and no tools were called.
pub fn maybe_summary_before_final_answer(
    workspace: &Path,
    run_id: Option<&str>,
    config: &ScratchpadConfig,
) -> Option<Message> {
    if !config.enabled {
        return None;
    }
    let store = open_store(workspace, run_id, None, None)?;
    if !inventory_complete(&store) {
        return None;
    }
    build_report_summary_message(workspace, Some(store.run_id()), config)
}

/// When audit scratchpad is active but P2 gates are unmet, nudge the model to keep tooling.
pub fn maybe_continue_incomplete_audit(
    workspace: &Path,
    run_id: Option<&str>,
    config: &ScratchpadConfig,
    messages: &[Message],
) -> Option<Message> {
    if !config.enabled {
        return None;
    }
    let store = open_store(workspace, run_id, None, None)?;
    if inventory_complete(&store) {
        return None;
    }

    let notes = store.read_notes().ok()?;
    let user_wants_report = messages.iter().any(|m| {
        m.role == "user"
            && m.content.iter().any(|block| {
                if let ContentBlock::Text { text, .. } = block {
                    user_prompt_triggers_report_summary(text, config)
                } else {
                    false
                }
            })
    });
    if notes.is_empty() && !user_wants_report {
        return None;
    }

    let inventory = store.read_inventory().ok()?;
    let stats = compute_coverage_stats(&inventory, &notes, config);
    let run_id = store.run_id();
    let l0 = build_l0_status_line(
        run_id,
        &stats,
        &resume_area_id_from_inventory(&inventory),
    );
    let pending = inventory
        .areas
        .iter()
        .filter(|a| a.status == AreaStatus::Pending)
        .count();

    let text = format!(
        "Audit scratchpad P2 incomplete — do **not** end this turn with prose-only output.\n\
         [{l0}]\n\
         Required before the report: (1) `scratchpad_import_agent` for any completed explore agents, \
         (2) `scratchpad_set_area` for all {pending} pending area(s) → `done` or `deferred` (meta note for deferred), \
         (3) sync `checklist_update` with inventory, \
         (4) `write_file` the audit report (verified findings only). \
         Call `scratchpad_status` now and continue with tools."
    );
    Some(Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text,
            cache_control: None,
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readonly_tool_detection() {
        assert!(is_readonly_tool("read_file"));
        assert!(!is_readonly_tool("write_file"));
        assert!(is_scratchpad_write_tool("scratchpad_append"));
    }

    #[test]
    fn report_keyword_match() {
        let cfg = ScratchpadConfig::default();
        assert!(user_prompt_triggers_report_summary("请写报告 synthesize", &cfg));
        assert!(user_prompt_triggers_report_summary(
            "帮我对项目进行代码级审核，输出md格式的报告",
            &cfg
        ));
        assert!(!user_prompt_triggers_report_summary("fix typo", &cfg));
    }

    #[test]
    fn report_summary_blocks_low_coverage() {
        use serde_json::json;

        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");
        let run_id = "gate-run";
        let base = ws.join(".deepseek/scratchpad").join(run_id);
        std::fs::create_dir_all(&base).expect("mkdir run");
        let inv = json!({
            "run_id": run_id,
            "areas": [
                {"id": "a1", "path": "p", "status": "pending", "notes": ""},
                {"id": "a2", "path": "p", "status": "pending", "notes": ""}
            ]
        });
        std::fs::write(
            base.join("inventory.json"),
            serde_json::to_string_pretty(&inv).unwrap(),
        )
        .expect("write inv");
        std::fs::write(base.join("notes.jsonl"), "").expect("notes");

        let cfg = ScratchpadConfig::default();
        let msg = build_report_summary_message(&ws, Some(run_id), &cfg).expect("msg");
        let text = match &msg.content[0] {
            ContentBlock::Text { text, .. } => text.as_str(),
            _ => panic!("text block"),
        };
        assert!(text.contains("BLOCKED"));
        assert!(!text.contains("[L1]"));
    }
}
