//! Unified tool write-state predicate (kernel-v2 M1).
//!
//! Merges the former `LoopGuard::is_state_mutating_tool` whitelist and
//! `tool_bridge::tool_name_is_mutating` heuristic into one conservative entry
//! point. Call sites treat `tool_writes_state(name)` as “this tool may change
//! workspace state”; M2 will swap the implementation to read `Footprint::writes`
//! without changing signatures.

/// Returns whether a tool may mutate durable workspace state.
///
/// Conservative union of the legacy loop-guard whitelist and the parallel-batch
/// mutating heuristic (`exec_shell` included). Intended to replace both prior
/// lists; when in doubt the predicate returns `true`.
#[must_use]
pub fn tool_writes_state(name: &str) -> bool {
    loop_guard_state_mutating_whitelist(name) || heuristic_writes_state(name)
}

/// Explicit file/dir mutators that reset identical-call loop-guard counters.
fn loop_guard_state_mutating_whitelist(name: &str) -> bool {
    matches!(
        name,
        "write_file"
            | "edit_file"
            | "apply_patch"
            | "create_dirs"
            | "batch_edit"
            | "refactor_imports"
    )
}

/// Substring / exact-name heuristic formerly in `tool_bridge`.
fn heuristic_writes_state(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("write")
        || lower.contains("edit")
        || lower.contains("patch")
        || lower.contains("delete")
        || lower == "exec_shell"
        || lower == "exec_shell_wait"
        || lower == "exec_shell_interact"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_includes_whitelist_and_heuristic_without_regression() {
        for name in ["write_file", "edit_file", "apply_patch", "create_dirs"] {
            assert!(
                tool_writes_state(name),
                "whitelist tool {name} must write state"
            );
        }
        for name in [
            "exec_shell",
            "exec_shell_wait",
            "exec_shell_interact",
            "write_office",
        ] {
            assert!(
                tool_writes_state(name),
                "heuristic tool {name} must write state"
            );
        }
        assert!(!tool_writes_state("read_file"));
        assert!(!tool_writes_state("grep_files"));
        assert!(!tool_writes_state("exec_shell_cancel"));
    }

    /// Golden snapshot: every built-in tool name → expected `tool_writes_state`.
    /// Update when adding tools; prevents union drift (M1 acceptance gate).
    #[test]
    fn golden_builtin_tool_writes_state() {
        const EXPECTED: &[(&str, bool)] = &[
            ("agent_assign", false),
            ("agent_cancel", false),
            ("agent_list", false),
            ("agent_result", false),
            ("agent_send_input", false),
            ("agent_spawn", false),
            ("agent_wait", false),
            ("apply_patch", true),
            ("batch_edit", true),
            ("assign_agent", false),
            ("automation_create", false),
            ("automation_list", false),
            ("automation_read", false),
            ("automation_run", false),
            ("automation_update", false),
            ("checklist_add", false),
            ("checklist_list", false),
            ("checklist_update", false),
            ("checklist_write", true),
            ("close_agent", false),
            ("code_execution", false),
            ("create_dirs", true),
            ("delegate_to_agent", false),
            ("describe_image", false),
            ("diagnostics", false),
            ("edit_file", true),
            ("exec_shell", true),
            ("exec_shell_cancel", false),
            ("exec_shell_interact", true),
            ("exec_shell_wait", true),
            ("fetch_url", false),
            ("file_info", false),
            ("file_search", false),
            ("fim_edit", true),
            ("finance", false),
            ("git_blame", false),
            ("git_diff", false),
            ("git_log", false),
            ("git_show", false),
            ("git_status", false),
            ("github_close_issue", false),
            ("github_comment", false),
            ("github_issue_context", false),
            ("github_pr_context", false),
            ("glob_files", false),
            ("grep_files", false),
            ("list_dir", false),
            ("list_mcp_resources", false),
            ("load_office_payload", false),
            ("load_skill", false),
            ("multi_tool_use.parallel", false),
            ("note", false),
            ("pr_attempt_list", false),
            ("pr_attempt_preflight", false),
            ("pr_attempt_read", false),
            ("pr_attempt_record", false),
            ("project_map", false),
            ("read_file", false),
            ("read_office", false),
            ("recall_archive", false),
            ("refactor_imports", true),
            ("remember", false),
            ("request_user_input", false),
            ("resume_agent", false),
            ("revert_turn", false),
            ("review", false),
            ("rlm", false),
            ("run_tests", false),
            ("scratchpad_append", false),
            ("scratchpad_import_agent", false),
            ("scratchpad_init", false),
            ("scratchpad_list_notes", false),
            ("scratchpad_set_area", false),
            ("scratchpad_status", false),
            ("scratchpad_verify_note", false),
            ("send_input", false),
            ("spawn_agent", false),
            ("task_cancel", false),
            ("task_create", false),
            ("task_gate_run", false),
            ("task_list", false),
            ("task_read", false),
            ("task_shell_start", false),
            ("task_shell_wait", false),
            ("todo_add", false),
            ("todo_list", false),
            ("todo_update", false),
            ("todo_write", true),
            ("tool_search_tool_bm25", false),
            ("tool_search_tool_regex", false),
            ("update_plan", false),
            ("validate_data", false),
            ("wait", false),
            ("web.run", false),
            ("web_search", false),
            ("write_file", true),
            ("write_office", true),
        ];

        for (name, expected) in EXPECTED {
            assert_eq!(
                tool_writes_state(name),
                *expected,
                "tool_writes_state({name}) drift — update golden or fix predicate"
            );
        }
    }
}
