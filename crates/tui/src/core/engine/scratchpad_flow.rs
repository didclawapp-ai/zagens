//! Audit scratchpad engine hooks (B3/B4/B3b).

use std::path::Path;

use crate::models::{ContentBlock, Message};
use crate::scratchpad::config::ScratchpadConfig;
use crate::scratchpad::AreaStatus;
use crate::scratchpad::{ScratchpadStore, build_layered_summary, display_run_path};

/// Per-step scratchpad nudge state (reset each engine step).
#[derive(Debug, Default)]
pub struct ScratchpadStepState {
    pub readonly_tool_successes: usize,
    pub scratchpad_writes_this_step: usize,
}

impl ScratchpadStepState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

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
    matches!(name, "scratchpad_append" | "scratchpad_set_area")
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

/// B3b — line for cycle handoff / structured state when a scratchpad exists.
#[must_use]
pub fn scratchpad_handoff_line(workspace: &Path, run_id: Option<&str>) -> Option<String> {
    let store = open_store(workspace, run_id, None, None)?;
    let status = store.build_status().ok()?;
    let resume = status
        .get("resume_area_id")
        .and_then(|v| v.as_str())
        .unwrap_or("none");
    Some(format!(
        "Active audit scratchpad: {} (resume_area_id: {resume})",
        display_run_path(store.run_id())
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

/// B3 — inject layered summary as a synthetic user message.
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
    let summary = build_layered_summary(&inventory, &notes, config.inject_summary_max_chars);
    let text = format!(
        "<scratchpad_summary run_id=\"{}\">\n{summary}\n</scratchpad_summary>",
        store.run_id()
    );
    Some(Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text,
            cache_control: None,
        }],
    })
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
        assert!(!user_prompt_triggers_report_summary("fix typo", &cfg));
    }
}
